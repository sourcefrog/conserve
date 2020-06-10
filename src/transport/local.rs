// Copyright 2020 Martin Pool.

//! Access to an archive on the local filesystem.

use std::fs::{create_dir, File};
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use crate::transport::{DirEntry, TransportRead, TransportWrite};

#[derive(Clone, Debug)]
pub struct LocalTransport {
    /// Root directory for this transport.
    root: PathBuf,
}

impl LocalTransport {
    pub fn new(path: &Path) -> Self {
        LocalTransport {
            root: path.to_owned(),
        }
    }

    pub fn full_path(&self, relpath: &str) -> PathBuf {
        debug_assert!(!relpath.contains("/../"), "path must not contain /../");
        self.root.join(relpath)
    }
}

impl TransportRead for LocalTransport {
    fn read_dir(
        &self,
        relpath: &str,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<DirEntry>>>> {
        // Archives should never normally contain non-UTF-8 (or even non-ASCII) filenames, but
        // let's pass them back as lossy UTF-8 so they can be reported at a higher level, for
        // example during validation.
        let relpath = relpath.to_owned();
        Ok(Box::new(self.full_path(&relpath).read_dir()?.map(
            move |i| {
                i.and_then(|de| {
                    let metadata = de.metadata()?;
                    Ok(DirEntry {
                        name: de.file_name().to_string_lossy().into(),
                        kind: de.file_type()?.into(),
                        len: metadata.len(),
                    })
                })
                .map_err(|e| e.into())
            },
        )))
    }

    fn read_file(&self, relpath: &str, out_buf: &mut Vec<u8>) -> io::Result<()> {
        out_buf.truncate(0);
        let len = File::open(&self.full_path(relpath))?.read_to_end(out_buf)?;
        out_buf.truncate(len);
        Ok(())
    }

    fn exists(&self, relpath: &str) -> io::Result<bool> {
        Ok(self.full_path(relpath).exists())
    }

    fn box_clone(&self) -> Box<dyn TransportRead> {
        Box::new(self.clone())
    }
}

impl TransportWrite for LocalTransport {
    fn create_dir(&mut self, relpath: &str) -> io::Result<()> {
        create_dir(self.full_path(&relpath)).or_else(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(err)
            }
        })
    }

    fn write_file(&mut self, relpath: &str, content: &[u8]) -> io::Result<()> {
        let full_path = self.full_path(relpath);
        let dir = full_path.parent().unwrap();
        let mut temp = tempfile::Builder::new()
            .prefix(crate::TMP_PREFIX)
            .tempfile_in(dir)?;
        if let Err(err) = temp.write_all(content) {
            let _ = temp.close();
            return Err(err);
        }
        if let Err(persist_error) = temp.persist(&full_path) {
            let _ = persist_error.file.close()?;
            Err(persist_error.error)
        } else {
            Ok(())
        }
    }

    fn box_clone_write(&self) -> Box<dyn TransportWrite> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod test {
    use assert_fs::prelude::*;
    use predicates::prelude::*;

    use super::*;
    use crate::kind::Kind;

    #[test]
    fn read_file() {
        let temp = assert_fs::TempDir::new().unwrap();
        let content: &str = "the ribs of the disaster";
        let filename = "poem.txt";

        temp.child(filename).write_str(content).unwrap();

        let transport = LocalTransport::new(temp.path());
        let mut buf = Vec::new();
        transport.read_file(&filename, &mut buf).unwrap();
        assert_eq!(buf, content.as_bytes());

        temp.close().unwrap();
    }

    #[test]
    fn read_with_non_empty_buffer() {
        let mut buf = b"already has some stuff".to_vec();
        let temp = assert_fs::TempDir::new().unwrap();
        let desired = b"content from file";
        let filename = "test.txt";
        temp.child(filename).write_binary(desired).unwrap();
        let transport = LocalTransport::new(temp.path());
        transport.read_file(&filename, &mut buf).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&buf),
            String::from_utf8_lossy(desired)
        );
        temp.close().unwrap();
    }

    #[test]
    fn list_directory() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("root file").touch().unwrap();
        temp.child("subdir").create_dir_all().unwrap();
        temp.child("subdir")
            .child("subfile")
            .write_str("Morning coffee")
            .unwrap();

        let transport = TransportRead::new(&temp.path().to_string_lossy()).unwrap();
        let mut root_list: Vec<_> = transport
            .read_dir(".")
            .unwrap()
            .map(io::Result::unwrap)
            .collect();
        assert_eq!(root_list.len(), 2);
        root_list.sort();

        assert_eq!(
            root_list[0],
            DirEntry {
                name: "root file".to_owned(),
                kind: Kind::File,
                len: 0,
            }
        );

        // Len is unpredictable for directories, so check the other fields.
        assert_eq!(root_list[1].name, "subdir");
        assert_eq!(root_list[1].kind, Kind::Dir);

        assert_eq!(transport.exists("root file").unwrap(), true);
        assert_eq!(transport.exists("nuh-uh").unwrap(), false);

        let subdir_list: Vec<_> = transport
            .read_dir("subdir")
            .unwrap()
            .map(io::Result::unwrap)
            .collect();
        assert_eq!(
            subdir_list,
            vec![DirEntry {
                name: "subfile".to_owned(),
                kind: Kind::File,
                len: 14,
            }]
        );

        temp.close().unwrap();
    }

    #[test]
    fn write_file() {
        // TODO: Maybe test some error cases of failing to write.
        let temp = assert_fs::TempDir::new().unwrap();
        let mut transport = TransportWrite::new(&temp.path().to_string_lossy()).unwrap();

        transport.create_dir("subdir").unwrap();
        transport
            .write_file("subdir/subfile", b"Must I paint you a picture?")
            .unwrap();

        temp.child("subdir").assert(predicate::path::is_dir());
        temp.child("subdir")
            .child("subfile")
            .assert("Must I paint you a picture?");

        temp.close().unwrap();
    }

    #[test]
    fn create_existing_dir() {
        let temp = assert_fs::TempDir::new().unwrap();
        let mut transport = TransportWrite::new(&temp.path().to_string_lossy()).unwrap();

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();

        temp.close().unwrap();
    }
}
