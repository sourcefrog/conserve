// Conserve backup system.
// Copyright 2015 Martin Pool.

/// Run external tests based on Cram in Python.

// Thanks to Zarathustra30 in http://stackoverflow.com/a/31760328/243712

use std::iter;
use std::env;
use std::path;

#[test]
fn run_cram () {
    use std::process::Command;

    // Allow stdout, stdenv from cram through to this test's descriptors, where they can be
    // captured by Cargo.
    
    // TODO: Better means to get the source root directory?
    // TODO: Glob the files ourselves to avoid dependency on sh?
    
    let old_path = &env::var("PATH").unwrap();
    let old_paths = env::split_paths(old_path);

    let mut addition = path::PathBuf::from(env::current_exe().unwrap());
    addition.pop();
    println!("addition {:?}", addition);

    let path = env::join_paths(iter::once(addition).chain(old_paths)).unwrap();
    println!("new path is: {:?}", path);
    
    match Command::new("sh")
        .env("PATH", path)
        .args(&["-c", "cram cramtests/*.md"])
        .status() {
        Ok(status) if status.success() => (),
        Err(e) => panic!("failed to run cram: {}", e),
        Ok(rc) => panic!("cram failed with status: {}", rc),
    }
}
