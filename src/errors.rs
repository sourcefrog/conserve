// Clippy warns about a redundant closure, but the closure is in the error-chain crate
// and not useful to flag here.
#![allow(unknown_lints,redundant_closure)]

use std::io;
use std::path::PathBuf;
use rustc_serialize;

error_chain! {
    foreign_links {
        Io(io::Error);
        JsonDecode(rustc_serialize::json::DecoderError);
    }

    errors {
        BlockCorrupt(block_hash: String) {
        }
        NotAnArchive(path: PathBuf) {
            display("Not a Conserve archive: {:?}", path)
        }
        UnsupportedArchiveVersion(version: String) {
            display("Unsupported archive version: {:?}", version)
        }
        DestinationNotEmpty(destination: PathBuf) {
            display("Destination directory not empty: {:?}", destination)
        }
        ArchiveEmpty {
            display("Archive is empty")
        }
        InvalidVersion {
            display("Invalid version number")
        }
    }
}
