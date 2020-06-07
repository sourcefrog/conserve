// Copyright 2020 Martin Pool.

//! Access to an archive on the local filesystem.

use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use crate::transport::{DirEntry, TransportRead};

pub struct LocalTransport {
    /// Root directory for this transport.
    root: PathBuf,

    /// Reusable buffer for reading data.
    read_buf: Vec<u8>,
}

impl LocalTransport {
    pub fn new(path: &Path) -> Self {
        LocalTransport {
            root: path.to_owned(),
            read_buf: Vec::new(),
        }
    }

    pub fn full_path(&self, relpath: &str) -> PathBuf {
        debug_assert!(!relpath.contains("/../"), "path must not contain /../");
        self.root.join(relpath)
    }
}

impl Clone for LocalTransport {
    fn clone(&self) -> Self {
        LocalTransport {
            root: self.root.clone(),
            read_buf: Vec::new(),
        }
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

    fn read_file(&mut self, relpath: &str) -> io::Result<&[u8]> {
        self.read_buf.truncate(0);
        File::open(&self.full_path(relpath))?.read_to_end(&mut self.read_buf)?;
        Ok(self.read_buf.as_slice())
    }

    fn exists(&self, relpath: &str) -> io::Result<bool> {
        Ok(self.full_path(relpath).exists())
    }

    fn box_clone(&self) -> Box<dyn TransportRead> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::kind::Kind;
    use assert_fs::prelude::*;

    #[test]
    fn read_file() {
        let temp = assert_fs::TempDir::new().unwrap();
        let content: &str = "the ribs of the disaster";
        let filename = "poem.txt";

        temp.child(filename).write_str(content).unwrap();

        let mut transport = LocalTransport::new(temp.path());
        assert_eq!(transport.read_file(&filename).unwrap(), content.as_bytes());

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

        let transport = LocalTransport::new(temp.path());
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
}
