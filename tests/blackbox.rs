// Conserve backup system.
// Copyright 2016 Martin Pool.

/// Run conserve CLI as a subprocess and test it.

// TODO: Maybe use https://github.com/dtolnay/indoc to make indented
// examples tidier.  But, not supported yet on stable.


use std::io;
use std::env;
use std::process;

extern crate tempdir;


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
    assert_success_and_output(&["--version"],
        "0.2.0\n", "");
}


#[test]
fn blackbox_help() {
    assert_success_and_output(
        &["--help"],
        "\
Conserve: an (incomplete) backup tool.
Copyright 2015, 2016 Martin Pool, GNU GPL v2+.
https://github.com/sourcefrog/conserve

Usage:
    conserve init <archivedir>
    conserve backup <archivedir> <source>...
    conserve --version
    conserve --help
",
        "");
}


#[test]
fn blackbox_init() {
    let testdir = make_tempdir();
    let mut arch_dir = testdir.path().to_path_buf();
    arch_dir.push("a");
    let args = ["init", arch_dir.to_str().unwrap()];
    let output = run_conserve(&args).unwrap();
    assert!(output.status.success());
    assert_eq!(0, output.stderr.len());
    assert!(String::from_utf8_lossy(&output.stdout)
        .starts_with("Created new archive"));
}


fn make_tempdir() -> tempdir::TempDir {
    tempdir::TempDir::new("conserve_blackbox").unwrap()
}


fn assert_success_and_output(args: &[&str], stdout: &str, stderr: &str) {
    let output = run_conserve(args).unwrap();
    assert!(output.status.success());
    assert_eq!(stderr, String::from_utf8_lossy(&output.stderr));
    assert_eq!(stdout, String::from_utf8_lossy(&output.stdout));
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
