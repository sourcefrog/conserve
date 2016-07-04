// Conserve backup system.
// Copyright 2016 Martin Pool.

/// Run conserve CLI and test it.


use std::io;
use std::iter;
use std::env;
use std::path;
use std::process;

#[test]
fn test_run_conserve_no_args() {
    // Run with no arguments, should fail with a usage message.
    let output = run_conserve().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let expected_out = "\
Invalid arguments.

Usage:
    conserve init <archivedir>
    conserve backup <archivedir> <source>...
    conserve --version
    conserve --help
".as_bytes();
    assert_eq!(expected_out, &output.stderr as &[u8]);
}

fn run_conserve() -> io::Result<process::Output> {
    // Allow stdout, stdenv from cram through to this test's descriptors, where they can be
    // captured by Cargo.

    // TODO: Better means to get the source root directory?
    // TODO: Clear path entirely?

    let old_path = &env::var("PATH").unwrap();
    let old_paths = env::split_paths(old_path);

    let exe_dir = env::current_exe().unwrap();
    let mut addition = path::PathBuf::from(exe_dir);
    addition.pop();
    println!("addition {:?}", addition);

    let path = env::join_paths(iter::once(addition).chain(old_paths)).unwrap();
    println!("new path is: {:?}", path);

    process::Command::new("conserve")
        .env("PATH", path)
        .output()
}
