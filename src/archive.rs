use std;
use std::fs::{File};
use std::io::{Error, Result, Write};
use std::path::{Path, PathBuf} ;

use rustc_serialize::json;

const HEADER_FILENAME: &'static str = "CONSERVE";
const ARCHIVE_VERSION: &'static str = "0.2.0";

#[derive(Debug)]
pub struct Archive {
    /// Top-level directory for the archive.
    dir: PathBuf,
}

#[derive(RustcDecodable, RustcEncodable)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

impl Archive {
    /// Make a new directory to hold an archive, and write the header.
    pub fn init(dir: &Path) -> Result<Archive> {
        debug!("Creating archive directory {:?}", dir.display());
        let archive = Archive {
            dir: dir.to_path_buf(),
        };
        if let Err(e) = std::fs::create_dir(&archive.dir) {
            error!("Failed to create archive directory {:?}: {}",
                archive.dir.display(), e);
            return Err(e);
        };
        if let Err(e) = archive.write_archive_header() {
            error!("Failed to write archive header: {}", e);
            return Err(e)
        };
        info!("Created new archive in {:?}", dir.display());
        Ok(archive)
    }

    fn write_archive_header(self: &Archive) -> Result<()> {
        let header = ArchiveHeader{
            conserve_archive_version: String::from(ARCHIVE_VERSION),
        };
        let header_path = self.dir.join(HEADER_FILENAME);
        let mut header_file = match File::create(&header_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Couldn't open archive header {:?}: {}",
                    header_path.display(), e);
                return Err(e)
            }
        };
        let header_json = json::encode(&header).unwrap();
        debug!("header json = {}", header_json);
        if let Err(e) = header_file.write_all(header_json.as_bytes()) {
            error!("Couldn't write header file {:?}: {}",
                header_path.display(), e);
            return Err(e)
        }
        Ok(())
    }

    pub fn path(self: &Archive) -> &Path {
        self.dir.as_path()
    }
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::*;

    #[test]
    fn test_create_archive() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch = Archive::init(&testdir.path().join("arch")).unwrap();

        assert_eq!(arch.path(), testdir.path().join("arch").as_path());
    }
}
