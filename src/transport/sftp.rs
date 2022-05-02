// Copyright 2022 Martin Pool

//! Read/write archive over SFTP.

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use tracing::{info, trace};
use url::Url;

use super::{DirEntry, Transport};
use crate::{Kind, Result};

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
        let tcp_stream = TcpStream::connect(&addr)?;
        trace!("got tcp connection");
        let mut session = ssh2::Session::new()?;
        session.set_tcp_stream(tcp_stream);
        session.handshake()?;
        trace!(
            "SSH hands shaken, banner: {}",
            session.banner().unwrap_or("(none)")
        );
        session.userauth_agent(url.username())?;
        trace!("Authenticated!");
        let sftp = session.sftp()?;
        Ok(SftpTransport {
            sftp: Arc::new(sftp),
            url: url.clone(),
            base_path: url.path().into(),
        })
    }

    fn lstat(&self, path: &str) -> io::Result<ssh2::FileStat> {
        trace!("lstat {path}");
        self.sftp
            .lstat(&self.base_path.join(path))
            .map_err(translate_error)
    }
}

impl fmt::Debug for SftpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SftpTransport")
            .field("url", &self.url)
            .finish()
    }
}

/// Map other errors to io::Error that aren't handled by libssh.
///
/// See https://github.com/alexcrichton/ssh2-rs/issues/244.
fn translate_error(err: ssh2::Error) -> io::Error {
    match err.code() {
        ssh2::ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_NO_SUCH_FILE)
        | ssh2::ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_NO_SUCH_PATH) => {
            io::Error::new(io::ErrorKind::NotFound, err)
        }
        _ => io::Error::from(err),
    }
}

impl Transport for SftpTransport {
    fn iter_dir_entries(
        &self,
        path: &str,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<super::DirEntry>>>> {
        let dir = self.sftp.opendir(&self.base_path.join(path))?;
        Ok(Box::new(ReadDir(dir)))
    }

    fn read_file(&self, path: &str) -> io::Result<Bytes> {
        let full_path = self.base_path.join(path);
        trace!("attempt open {}", full_path.display());
        let mut buf = Vec::with_capacity(2 << 20);
        let mut file = self.sftp.open(&full_path).map_err(translate_error)?;
        let len = file.read_to_end(&mut buf)?;
        assert_eq!(len, buf.len());
        trace!("read {} bytes from {}", len, full_path.display());
        Ok(buf.into())
    }

    fn is_dir(&self, path: &str) -> io::Result<bool> {
        Ok(self.lstat(path)?.is_dir())
    }

    fn is_file(&self, path: &str) -> io::Result<bool> {
        Ok(self.lstat(path)?.is_file())
    }

    fn create_dir(&self, relpath: &str) -> io::Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!("create_dir {:?}", full_path);
        self.sftp.mkdir(&full_path, 0o700).map_err(translate_error)
    }

    fn write_file(&self, relpath: &str, content: &[u8]) -> io::Result<()> {
        let full_path = self.base_path.join(relpath);
        trace!("write_file {:>9} bytes to {:?}", content.len(), full_path);
        let mut file = self.sftp.create(&full_path).map_err(translate_error)?;
        file.write_all(content).map_err(|err| {
            debug!("sftp error {err:?} writing {full_path:?}");
            err
        })
    }

    fn metadata(&self, _relpath: &str) -> io::Result<super::Metadata> {
        todo!("metadata")
    }

    fn remove_file(&self, _relpath: &str) -> io::Result<()> {
        todo!("remove_file")
    }

    fn remove_dir(&self, _relpath: &str) -> io::Result<()> {
        todo!("remove_dir")
    }

    fn remove_dir_all(&self, _relpath: &str) -> io::Result<()> {
        todo!("remove_dir_all")
    }

    fn sub_transport(&self, relpath: &str) -> Box<dyn Transport> {
        let base_path = self.base_path.join(relpath);
        let mut url = self.url.clone();
        url.set_path(base_path.to_str().unwrap());
        Box::new(SftpTransport {
            url,
            sftp: Arc::clone(&self.sftp),
            base_path,
        })
    }

    fn url_scheme(&self) -> &'static str {
        "sftp"
    }
}

struct ReadDir(ssh2::File);

impl Iterator for ReadDir {
    type Item = io::Result<super::DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.0.readdir() {
                Ok((pathbuf, file_stat)) => {
                    let name = pathbuf.to_string_lossy().into();
                    if name == "." || name == ".." {
                        continue;
                    }
                    return Some(Ok(DirEntry {
                        name,
                        kind: file_stat.file_type().into(),
                    }));
                }
                Err(err) if err.code() == ssh2::ErrorCode::Session(-16) => {
                    // Apparently there's no symbolic version for it, but this is the error
                    // code.
                    // <https://github.com/alexcrichton/ssh2-rs/issues/140>
                    return None;
                }
                Err(err) => {
                    info!("SFTP error {:?}", err);
                    return Some(Err(err.into()));
                }
            }
        }
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
