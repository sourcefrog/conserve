use std;
use std::io::{Error, ErrorKind};

pub struct Archive {
    /// Top-level directory for the archive.
    dir: String,
}

impl Archive {
    /// Make a new directory to hold an archive, and write the header.
    pub fn init(dir: &str) -> std::io::Result<Archive> {
        error!("Create archive directory {}", dir);
        println!("init called");
        Ok(Archive { dir: dir.to_string() })
    }
}
