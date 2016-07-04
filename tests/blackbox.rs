// Conserve backup system.
// Copyright 2016 Martin Pool.

/// Run conserve CLI and test it.


use std::io;
use std::env;
use std::process;

#[test]
fn blackbox_no_args() {
    // Run with no arguments, should fail with a usage message.
    let output = run_conserve(&[]).unwrap();
    assert_eq!(output.status.code(), Some(1));
    let expected_out = "\
Invalid arguments.

Usage:
    conserve init <archivedir>
    conserve backup <archivedir> <source>...
    conserve --version
    conserve --help
";
    assert_eq!(expected_out, String::from_utf8_lossy(&output.stderr));
}

#[test]
fn blackbox_version() {
    let output = run_conserve(&["--version"]).unwrap();
    assert!(output.status.success());
    assert_eq!("0.2.0\n", String::from_utf8_lossy(&output.stdout));
    assert_eq!("", String::from_utf8_lossy(&output.stderr));
}
/// Run Conserve's binary and return the status and output as strings.
fn run_conserve(args: &[&str]) -> io::Result<process::Output> {
    // Allow stdout, stdenv from cram through to this test's descriptors, where they can be
    // captured by Cargo.

    let mut conserve_path = env::current_exe().unwrap().to_path_buf();
    conserve_path.pop();
    conserve_path.push("conserve");

    process::Command::new(&conserve_path)
        .args(args)
        .env_clear()
        .output()
}
