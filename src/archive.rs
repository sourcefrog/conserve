use std;
use std::fs::{File};
use std::io::{Error, ErrorKind, Result, Write};
use std::path::PathBuf;

use rustc_serialize::json;

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
    pub fn init(dir: &str) -> Result<Archive> {
        info!("Creating archive directory {}", dir);
        let archive = Archive {
            dir: PathBuf::from(dir),
        };
        match std::fs::create_dir(&archive.dir) {
            Err(e) => {
                error!("Failed to create archive directory {:?}: {}",
                    archive.dir.display(), e);
                return Err(e);
            },
            Ok(_) => (),
        }
        
        match archive.write_archive_header() {
            Err(e) => {
                error!("Failed to write archive header: {}", e);
                Err(e)
            },
            Ok(_) => Ok(archive),
        }
    }

    fn write_archive_header(self: &Archive) -> Result<()> {
        let header = ArchiveHeader{
            conserve_archive_version: String::from(ARCHIVE_VERSION),
        };
        let header_path = self.dir.join("conserve-archive");
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
        match header_file.write_all(header_json.as_bytes()) {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("Couldn't write header file {:?}: {}",
                    header_path.display(), e);
                Err(e)
            }
        }
    }
}
