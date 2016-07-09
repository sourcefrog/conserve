// Conserve backup system.
// Copyright 2016 Martin Pool.

/// Run conserve CLI as a subprocess and test it.


use std::env;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process;

extern crate tempdir;


/// Strip from every line, the amount of indentation on the first line.
///
/// (Spaces only, no tabs.)
fn strip_indents(s: &str) -> String {
    let mut indent = 0;
    // Skip initial newline.
    for line in s[1..].split('\n') {
        for ch in line.chars() {
            if ch == ' ' {
                indent += 1;
            } else {
                break;
            }
        }
        break;
    }
    assert!(indent > 0);
    let mut r = String::new();
    let mut first = true;
    for line in s[1..].split('\n') {
        if !first {
            r.push('\n');
        }
        if line.len() > indent {
            r.push_str(&line[indent..]);
        }
        first = false;
    }
    r
}


#[test]
fn blackbox_no_args() {
    // Run with no arguments, should fail with a usage message.
    let output = run_conserve(&[]);
    assert_eq!(output.status.code(), Some(1));
    let expected_out = strip_indents("
        Invalid arguments.

        Usage:
            conserve init <archive>
            conserve backup <archive> <source>
            conserve --version
            conserve --help
        ");
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
        &strip_indents("
            Conserve: an (incomplete) backup tool.
            Copyright 2015, 2016 Martin Pool, GNU GPL v2+.
            https://github.com/sourcefrog/conserve

            Usage:
                conserve init <archive>
                conserve backup <archive> <source>
                conserve --version
                conserve --help
            "),
        "");
}


#[test]
fn blackbox_init() {
    let testdir = make_tempdir();
    let arch_dir = testdir.path().join("a");
    let args = ["init", arch_dir.to_str().unwrap()];
    let output = run_conserve(&args);
    assert!(output.status.success());
    assert_eq!(0, output.stderr.len());
    assert!(String::from_utf8_lossy(&output.stdout)
        .starts_with("Created new archive"));
}



#[test]
fn blackbox_backup() {
    let testdir = make_tempdir();
    let arch_dir = testdir.path().join("a");
    let arch_dir_str = arch_dir.to_str().unwrap();
    let output = run_conserve(&["init", &arch_dir_str]);
    assert!(output.status.success());

    let src = TreeFixture::new();
    src.create_file("hello");

    let output = run_conserve(&["backup", &arch_dir_str, src.root.to_str().unwrap()]);
    assert!(output.status.success());
    // TODO: Inspect the archive
}


fn make_tempdir() -> tempdir::TempDir {
    tempdir::TempDir::new("conserve_blackbox").unwrap()
}


fn assert_success_and_output(args: &[&str], stdout: &str, stderr: &str) {
    let output = run_conserve(args);
    assert!(output.status.success());
    assert_eq!(stderr, String::from_utf8_lossy(&output.stderr));
    assert_eq!(stdout, String::from_utf8_lossy(&output.stdout));
}


/// Run Conserve's binary and return a `process::Output` including its return code, stdout
/// and stderr text.
fn run_conserve(args: &[&str]) -> process::Output {
    let mut conserve_path = env::current_exe().unwrap().to_path_buf();
    conserve_path.pop();  // Remove name of test binary
    conserve_path.push("conserve");
    match process::Command::new(&conserve_path).args(args).output() {
        Ok(p) => p,
        Err(e) => {
            panic!("Failed to run conserve: {}", e);
        }
    }
}



/// A temporary tree for running a test.
///
/// Created in a temporary directory and automatically disposed when done.
pub struct TreeFixture {
    pub root: PathBuf,
    _tempdir: tempdir::TempDir, // held only for cleanup
}

impl TreeFixture {
    pub fn new() -> TreeFixture {
        let tempdir = tempdir::TempDir::new("conserve_TreeFixture").unwrap();
        let root = tempdir.path().to_path_buf();
        TreeFixture {
            _tempdir: tempdir,
            root: root,
        }
    }

    pub fn path(self: &TreeFixture) -> &Path {
        &self.root
    }

    pub fn create_file(self: &TreeFixture, relative_path: &str) {
        let full_path = self.root.join(relative_path);
        let mut f = std::fs::File::create(full_path).unwrap();
        f.write_all("contents".as_bytes()).unwrap();
    }
}
