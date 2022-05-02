// Copyright 2022 Martin Pool

//! Read/write archive over SFTP.

use std::fmt;
use std::io::{self, Read};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use bytes::Bytes;
use url::Url;

use super::{DirEntry, Transport};
use crate::{Kind, Result};

/// Archive file I/O over SFTP.
#[derive(Clone)]
pub struct SftpTransport {
    url: Url,
    sftp: Arc<Mutex<ssh2::Sftp>>,
    base_path: PathBuf,
}

impl SftpTransport {
    pub fn new(url: &Url) -> Result<SftpTransport> {
        assert_eq!(url.scheme(), "sftp");
        assert!(url.host_str().is_some());
        let addr = format!(
            "{}:{}",
            url.host_str().expect("url must have a host"),
            url.port().unwrap_or(22)
        );
        let tcp_stream = TcpStream::connect(&addr)?;
        eprintln!("got tcp connection");
        let mut session = ssh2::Session::new()?;
        session.set_tcp_stream(tcp_stream);
        session.handshake()?;
        eprintln!("SSH hands shaken");
        eprintln!("banner>> {}", session.banner().unwrap_or("(none)"));
        session.userauth_agent(url.username())?;
        eprintln!("Authenticated!");
        let sftp = session.sftp()?;
        Ok(SftpTransport {
            sftp: Arc::new(Mutex::new(sftp)),
            url: url.clone(),
            base_path: url.path().into(),
        })
    }

    fn lstat(&self, path: &str) -> io::Result<ssh2::FileStat> {
        Ok(self
            .sftp
            .lock()
            .unwrap()
            .lstat(&self.base_path.join(path))?)
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
    fn iter_dir_entries(
        &self,
        path: &str,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<super::DirEntry>>>> {
        let dir = self
            .sftp
            .lock()
            .unwrap()
            .opendir(&self.base_path.join(path))?;
        Ok(Box::new(ReadDir(dir)))
    }

    fn read_file(&self, path: &str) -> io::Result<Bytes> {
        let sftp_lock = self.sftp.lock().unwrap();
        let mut buf = Vec::with_capacity(2 << 20);
        let mut file = sftp_lock.open(&self.base_path.join(path))?;
        let len = file.read_to_end(&mut buf)?;
        assert_eq!(len, buf.len());
        Ok(buf.into())
    }

    fn is_dir(&self, path: &str) -> io::Result<bool> {
        Ok(self.lstat(path)?.is_dir())
    }

    fn is_file(&self, path: &str) -> io::Result<bool> {
        Ok(self.lstat(path)?.is_file())
    }

    fn create_dir(&self, _relpath: &str) -> io::Result<()> {
        todo!()
    }

    fn write_file(&self, _relpath: &str, _content: &[u8]) -> io::Result<()> {
        todo!()
    }

    fn metadata(&self, _relpath: &str) -> io::Result<super::Metadata> {
        todo!()
    }

    fn remove_file(&self, _relpath: &str) -> io::Result<()> {
        todo!()
    }

    fn remove_dir(&self, _relpath: &str) -> io::Result<()> {
        todo!()
    }

    fn remove_dir_all(&self, _relpath: &str) -> io::Result<()> {
        todo!()
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
                    eprintln!("{:?}", err);
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
