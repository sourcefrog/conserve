// Copyright 2022-2025 Martin Pool

//! Read/write archive over SFTP.

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use time::OffsetDateTime;
use tokio::task::spawn_blocking;
use tracing::{error, info, instrument, trace, warn};
use url::Url;

use crate::Kind;

use super::{Error, ErrorKind, ListDir, Result, WriteMode};

pub(super) struct Protocol {
    url: Url,
    // TODO: Maybe use a Tokio Mutex here, so that threads don't get hung up on the mutex inside
    // the C library.
    //
    // TODO: In fact perhaps have a pool of connections to get more parallelism?
    sftp: Arc<ssh2::Sftp>,
    base_path: PathBuf,
}

impl Protocol {
    pub fn new(url: &Url) -> Result<Self> {
        assert_eq!(url.scheme(), "sftp");
        let addr = format!(
            "{}:{}",
            url.host_str().expect("url must have a host"),
            url.port().unwrap_or(22)
        );
        let tcp_stream = TcpStream::connect(addr).map_err(|err| {
            error!(?err, ?url, "Error opening SSH TCP connection");
            io_error(err, url)
        })?;
        trace!("got tcp connection");
        let mut session = ssh2::Session::new().map_err(|err| {
            error!(?err, "Error opening SSH session");
            ssh_error(err, url)
        })?;
        session.set_tcp_stream(tcp_stream);
        session.handshake().map_err(|err| {
            error!(?err, "Error in SSH handshake");
            ssh_error(err, url)
        })?;
        trace!(
            "SSH hands shaken, banner: {}",
            session.banner().unwrap_or("(none)")
        );
        let username = match url.username() {
            "" => {
                trace!("Take default SSH username from environment");
                whoami::username()
            }
            u => u.to_owned(),
        };
        session.userauth_agent(&username).map_err(|err| {
            error!(?err, username, "Error in SSH user auth with agent");
            ssh_error(err, url)
        })?;
        trace!("Authenticated!");
        let sftp = session.sftp().map_err(|err| {
            error!(?err, "Error opening SFTP session");
            ssh_error(err, url)
        })?;
        Ok(Protocol {
            url: url.to_owned(),
            sftp: Arc::new(sftp),
            base_path: url.path().into(),
        })
    }

    fn lstat(&self, path: &str) -> Result<ssh2::FileStat> {
        trace!("lstat {path}");
        self.sftp
            .lstat(&self.base_path.join(path))
            .map_err(|err| self.ssh_error(err, path))
    }

    fn join_url(&self, path: &str) -> Url {
        self.url.join(path).expect("join URL")
    }

    fn ssh_error(&self, source: ssh2::Error, path: &str) -> Error {
        ssh_error(source, &self.join_url(path))
    }
}

#[async_trait]
impl super::Protocol for Protocol {
    async fn list_dir_async(&self, path: &str) -> Result<ListDir> {
        let sftp = self.sftp.clone();
        let url = self.join_url(path);
        let full_path = self.base_path.join(path);
        spawn_blocking(|| list_dir(sftp, full_path, url))
            .await
            .expect("spawn_blocking")
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let full_path = self.base_path.join(path);
        let url = &self.url.join(path).expect("join URL");
        trace!("read {url}");
        let mut buf = Vec::with_capacity(2 << 20);
        let mut file = self
            .sftp
            .open(&full_path)
            .map_err(|err| self.ssh_error(err, path))?;
        let len = file
            .read_to_end(&mut buf)
            .map_err(|err| io_error(err, url))?;
        assert_eq!(len, buf.len());
        trace!("read {} bytes from {}", len, full_path.display());
        Ok(buf.into())
    }

    async fn create_dir(&self, relpath: &str) -> Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!("create_dir {:?}", full_path);
        let sftp = Arc::clone(&self.sftp);
        match spawn_blocking(move || sftp.mkdir(&full_path, 0o700))
            .await
            .expect("spawn_blocking")
        {
            Ok(()) => Ok(()),
            Err(err) if err.code() == ssh2::ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_FAILURE) => {
                // openssh seems to say failure for "directory exists" :/
                Ok(())
            }
            Err(err) => {
                warn!(?err, ?relpath);
                Err(self.ssh_error(err, relpath))
            }
        }
    }

    fn write(&self, relpath: &str, content: &[u8], write_mode: WriteMode) -> Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!(
            "write_file {len:>9} bytes to {full_path:?}",
            len = content.len()
        );
        let flags = ssh2::OpenFlags::WRITE
            | ssh2::OpenFlags::CREATE
            | match write_mode {
                WriteMode::CreateNew => ssh2::OpenFlags::EXCLUSIVE,
                WriteMode::Overwrite => ssh2::OpenFlags::TRUNCATE,
            };
        let mut file = self
            .sftp
            .open_mode(&full_path, flags, 0o644, ssh2::OpenType::File)
            .map_err(|err| {
                warn!(?err, ?relpath, "sftp error creating file");
                self.ssh_error(err, relpath)
            })?;
        if let Err(err) = file.write_all(content) {
            warn!(?err, ?full_path, "sftp error writing file");
            if let Err(err2) = self.sftp.unlink(&full_path) {
                warn!(
                    ?err2,
                    ?full_path,
                    "sftp error unlinking file after write error"
                );
            }
            return Err(super::Error {
                url: Some(self.join_url(relpath)),
                source: Some(Box::new(err)),
                kind: ErrorKind::Other,
            });
        }
        Ok(())
    }

    async fn metadata(&self, relpath: &str) -> Result<super::Metadata> {
        let full_path = self.base_path.join(relpath);
        let stat = self.lstat(relpath)?;
        trace!("metadata {full_path:?}");
        let modified = stat.mtime.ok_or_else(|| {
            warn!("No mtime for {full_path:?}");
            super::Error {
                kind: ErrorKind::Other,
                source: None,
                url: Some(self.join_url(relpath)),
            }
        })?;
        let modified = OffsetDateTime::from_unix_timestamp(modified as i64).map_err(|err| {
            warn!("Invalid mtime for {full_path:?}");
            super::Error {
                kind: ErrorKind::Other,
                source: Some(Box::new(err)),
                url: Some(self.join_url(relpath)),
            }
        })?;
        Ok(super::Metadata {
            kind: stat.file_type().into(),
            len: stat.size.unwrap_or_default(),
            modified,
        })
    }

    fn remove_file(&self, relpath: &str) -> Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!("remove_file {full_path:?}");
        self.sftp
            .unlink(&full_path)
            .map_err(|err| self.ssh_error(err, relpath))
    }

    #[instrument]
    fn remove_dir_all(&self, path: &str) -> Result<()> {
        let mut dirs_to_walk = vec![path.to_owned()];
        let mut dirs_to_delete = vec![path.to_owned()];
        while let Some(dir) = dirs_to_walk.pop() {
            trace!(?dir, "Walk down dir");
            let list = list_dir(
                self.sftp.clone(),
                self.base_path.join(path),
                self.join_url(path),
            )?;
            for file in list.files {
                self.remove_file(&format!("{dir}/{file}"))?;
            }
            list.dirs
                .iter()
                .map(|subdir| format!("{dir}/{subdir}"))
                .for_each(|p| {
                    dirs_to_delete.push(p.clone());
                    dirs_to_walk.push(p)
                });
        }
        // Consume them in the reverse order discovered, so bottom up
        for dir in dirs_to_delete.iter().rev() {
            let full_path = self.base_path.join(dir);
            trace!(?dir, "rmdir");
            self.sftp
                .rmdir(&full_path)
                .map_err(|err| self.ssh_error(err, dir))?;
        }
        Ok(())
    }

    fn chdir(&self, relpath: &str) -> Arc<dyn super::Protocol> {
        let base_path = self.base_path.join(relpath);
        let url = self.url.join(relpath).expect("join URL");
        Arc::new(Protocol {
            url,
            sftp: Arc::clone(&self.sftp),
            base_path,
        })
    }

    fn url(&self) -> &Url {
        &self.url
    }
}

impl fmt::Debug for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("sftp::Protocol")
            .field("url", &self.url)
            .finish()
    }
}

impl From<ssh2::FileType> for Kind {
    fn from(kind: ssh2::FileType) -> Self {
        use ssh2::FileType::*;
        match kind {
            RegularFile => Kind::File,
            Directory => Kind::Dir,
            Symlink => Kind::Symlink,
            _ => Kind::Unknown,
        }
    }
}

impl From<ssh2::ErrorCode> for ErrorKind {
    fn from(code: ssh2::ErrorCode) -> Self {
        // Map other errors to io::Error that aren't handled by libssh.
        //
        // See https://github.com/alexcrichton/ssh2-rs/issues/244.
        match code {
            ssh2::ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_NO_SUCH_FILE)
            | ssh2::ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_NO_SUCH_PATH) => ErrorKind::NotFound,
            // TODO: Others
            _ => ErrorKind::Other,
        }
    }
}

fn ssh_error(source: ssh2::Error, url: &Url) -> super::Error {
    super::Error {
        kind: source.code().into(),
        source: Some(Box::new(source)),
        url: Some(url.clone()),
    }
}

fn io_error(source: io::Error, url: &Url) -> Error {
    Error {
        kind: source.kind().into(),
        source: Some(Box::new(source)),
        url: Some(url.clone()),
    }
}

fn list_dir(sftp: Arc<ssh2::Sftp>, full_path: PathBuf, url: Url) -> Result<ListDir> {
    trace!(?full_path, "list_dir");
    let mut dir = sftp.opendir(&full_path).map_err(|err| {
        error!(?err, ?full_path, "Error opening directory");
        ssh_error(err, &url)
    })?;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    loop {
        match dir.readdir() {
            Ok((pathbuf, file_stat)) => {
                let name = pathbuf.to_string_lossy().into();
                if name == "." || name == ".." {
                    continue;
                }
                trace!("read dir got name {}", name);
                match file_stat.file_type().into() {
                    Kind::File => files.push(name),
                    Kind::Dir => dirs.push(name),
                    _ => (),
                }
            }
            Err(err) if err.code() == ssh2::ErrorCode::Session(-16) => {
                // Apparently there's no symbolic version for it, but this is the error
                // code.
                // <https://github.com/alexcrichton/ssh2-rs/issues/140>
                trace!("read dir end");
                break;
            }
            Err(err) => {
                info!("SFTP error {err:?}");
                return Err(ssh_error(err, &url));
            }
        }
    }
    Ok(ListDir { files, dirs })
}
