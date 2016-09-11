use std::io;
use std::path::PathBuf;
use rustc_serialize;

error_chain! {
    foreign_links {
        io::Error, Io;
        rustc_serialize::json::DecoderError, JsonDecode;
    }

    errors {
        BlockCorrupt(block_hash: String) {
        }
        NotAnArchive(path: PathBuf) {
        }
        UnsupportedArchiveVersion(version: String) {
            display("unsupported archive version: {:?}", version)
        }
    }
}
