use std;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Archive {
    /// Top-level directory for the archive.
    dir: PathBuf,
}

impl Archive {
    /// Make a new directory to hold an archive, and write the header.
    pub fn init(dir: &str) -> std::io::Result<Archive> {
        info!("Creating archive directory {}", dir);
        let pathbuf = PathBuf::from(dir);
        try!(std::fs::create_dir(&pathbuf));
        Ok(Archive {
            dir: pathbuf,
        })
    }
}
