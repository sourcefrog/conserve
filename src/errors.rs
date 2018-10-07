// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Conserve error types.

use rustc_serialize;
use std::io;
use std::path::PathBuf;

use BandId;

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
        NoCompleteBands {
            display("Archive has no complete bands")
        }
        InvalidVersion {
            display("Invalid version number")
        }
        BandIncomplete(band_id: BandId) {
            display("Band {} is incomplete", band_id)
        }
    }
}
