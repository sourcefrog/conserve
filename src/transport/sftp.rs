// Copyright 2022 Martin Pool

//! Read/write archive over SFTP.

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use tracing::{error, info, trace, warn};
use url::Url;

use crate::Kind;

use super::{Error, ErrorKind, ListDir, Result, Transport};

pub(super) struct Protocol {
    transport: SftpTransport,
}

impl Protocol {
    pub fn new(url: &Url) -> Result<Self> {
        Ok(Protocol {
            transport: SftpTransport::new(url)?,
        })
    }
}

impl super::Protocol for Protocol {
    fn read_file(&self, relpath: &str) -> Result<Bytes> {
        self.transport.read_file(relpath)
    }

    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()> {
        todo!()
    }

    fn list_dir(&self, relpath: &str) -> Result<ListDir> {
        todo!()
    }

    fn create_dir(&self, relpath: &str) -> Result<()> {
        todo!()
    }

    fn metadata(&self, relpath: &str) -> Result<super::Metadata> {
        todo!()
    }

    fn remove_file(&self, relpath: &str) -> Result<()> {
        todo!()
    }

    fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        todo!()
    }

    fn chdir(&self, relpath: &str) -> Arc<dyn super::Protocol> {
        todo!()
    }

    fn url(&self) -> &Url {
        todo!()
    }
}

/// Archive file I/O over SFTP.
#[derive(Clone)]
pub struct SftpTransport {
    url: Url,
    sftp: Arc<ssh2::Sftp>,
    base_path: PathBuf,
}

impl SftpTransport {
    pub fn new(url: &Url) -> Result<SftpTransport> {
        assert_eq!(url.scheme(), "sftp");
        let addr = format!(
            "{}:{}",
            url.host_str().expect("url must have a host"),
            url.port().unwrap_or(22)
        );
        let tcp_stream = TcpStream::connect(addr).map_err(|err| {
            error!(?err, ?url, "Error opening SSH TCP connection");
            io_error(err, url.as_ref())
        })?;
        trace!("got tcp connection");
        let mut session = ssh2::Session::new().map_err(|err| {
            error!(?err, "Error opening SSH session");
            ssh_error(err, url.as_ref())
        })?;
        session.set_tcp_stream(tcp_stream);
        session.handshake().map_err(|err| {
            error!(?err, "Error in SSH handshake");
            ssh_error(err, url.as_ref())
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
            ssh_error(err, url.as_ref())
        })?;
        trace!("Authenticated!");
        let sftp = session.sftp().map_err(|err| {
            error!(?err, "Error opening SFTP session");
            ssh_error(err, url.as_ref())
        })?;
        Ok(SftpTransport {
            sftp: Arc::new(sftp),
            url: url.clone(),
            base_path: url.path().into(),
        })
    }

    fn lstat(&self, path: &str) -> Result<ssh2::FileStat> {
        trace!("lstat {path}");
        self.sftp
            .lstat(&self.base_path.join(path))
            .map_err(|err| ssh_error(err, path))
    }
}

impl fmt::Debug for SftpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SftpTransport")
            .field("url", &self.url)
            .finish()
    }
}

impl Transport for SftpTransport {
    fn list_dir(&self, path: &str) -> Result<ListDir> {
        let full_path = &self.base_path.join(path);
        trace!("iter_dir_entries {:?}", full_path);
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let mut dir = self.sftp.opendir(full_path).map_err(|err| {
            error!(?err, ?full_path, "Error opening directory");
            ssh_error(err, full_path.to_string_lossy().as_ref())
        })?;
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
                    info!("SFTP error {:?}", err);
                    return Err(ssh_error(err, path));
                }
            }
        }
        Ok(ListDir { files, dirs })
    }

    fn read_file(&self, path: &str) -> Result<Bytes> {
        let full_path = self.base_path.join(path);
        trace!("attempt open {}", full_path.display());
        let mut buf = Vec::with_capacity(2 << 20);
        let mut file = self
            .sftp
            .open(&full_path)
            .map_err(|err| ssh_error(err, path))?;
        let len = file
            .read_to_end(&mut buf)
            .map_err(|err| io_error(err, path))?;
        assert_eq!(len, buf.len());
        trace!("read {} bytes from {}", len, full_path.display());
        Ok(buf.into())
    }

    fn create_dir(&self, relpath: &str) -> Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!("create_dir {:?}", full_path);
        match self.sftp.mkdir(&full_path, 0o700) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == ssh2::ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_FAILURE) => {
                // openssh seems to say failure for "directory exists" :/
                Ok(())
            }
            Err(err) => {
                warn!(?err, ?relpath);
                Err(ssh_error(err, relpath))
            }
        }
    }

    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!("write_file {:>9} bytes to {:?}", content.len(), full_path);
        let mut file = self.sftp.create(&full_path).map_err(|err| {
            warn!(?err, ?relpath, "sftp error creating file");
            ssh_error(err, relpath)
        })?;
        file.write_all(content).map_err(|err| {
            warn!(?err, ?full_path, "sftp error writing file");
            io_error(err, relpath)
        })
    }

    fn metadata(&self, relpath: &str) -> Result<super::Metadata> {
        let full_path = self.base_path.join(relpath);
        let stat = self.lstat(relpath)?;
        trace!("metadata {full_path:?}");
        Ok(super::Metadata {
            kind: stat.file_type().into(),
            len: stat.size.unwrap_or_default(),
        })
    }

    fn remove_file(&self, relpath: &str) -> Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!("remove_file {full_path:?}");
        self.sftp
            .unlink(&full_path)
            .map_err(|err| ssh_error(err, relpath))
    }

    fn remove_dir_all(&self, path: &str) -> Result<()> {
        trace!(?path, "SftpTransport::remove_dir_all");
        let mut dirs_to_walk = vec![path.to_owned()];
        let mut dirs_to_delete = vec![path.to_owned()];
        while let Some(dir) = dirs_to_walk.pop() {
            trace!(?dir, "Walk down dir");
            let list = self.list_dir(&dir)?;
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
                .map_err(|err| ssh_error(err, dir))?;
        }
        Ok(())
        // let full_path = self.base_path.join(relpath);
        // trace!("remove_dir {full_path:?}");
        // self.sftp.rmdir(&full_path).map_err(translate_error)
    }

    fn sub_transport(&self, relpath: &str) -> Arc<dyn Transport> {
        let base_path = self.base_path.join(relpath);
        let mut url = self.url.clone();
        url.set_path(base_path.to_str().unwrap());
        Arc::new(SftpTransport {
            url,
            sftp: Arc::clone(&self.sftp),
            base_path,
        })
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

fn ssh_error(source: ssh2::Error, path: &str) -> super::Error {
    super::Error {
        kind: source.code().into(),
        source: Some(Box::new(source)),
        path: Some(path.to_owned()),
    }
}

fn io_error(source: io::Error, path: &str) -> Error {
    Error {
        kind: source.kind().into(),
        source: Some(Box::new(source)),
        path: Some(path.to_owned()),
    }
}
