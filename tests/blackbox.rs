// Conserve backup system.
// Copyright 2016 Martin Pool.

/// Run conserve CLI as a subprocess and test it.


use std::env;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process;
use std::str;

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
    let (status, stdout, stderr) = run_conserve(&[]);
    assert_eq!(status.code(), Some(1));
    let expected_err = strip_indents("
        Invalid arguments.

        Usage:
            conserve init [options] <archive>
            conserve backup [options] <archive> <source>
            conserve list-versions [options] <archive>
            conserve list-source [options] <source>
            conserve --version
            conserve --help
        ");
    assert_eq!(expected_err, stderr);
    assert_eq!("", stdout);
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
                conserve init [options] <archive>
                conserve backup [options] <archive> <source>
                conserve list-versions [options] <archive>
                conserve list-source [options] <source>
                conserve --version
                conserve --help

            Options:
                --stats         Show statistics at completion.
            "),
        "");
}


#[test]
fn clean_error_on_non_archive() {
    // Try to backup into a directory that is not an archive.
    let testdir = make_tempdir();
    let not_archive_path_str = testdir.path().to_str().unwrap();
    let (status, stdout, stderr) = run_conserve(&["backup", &not_archive_path_str, "."]);
    // TODO: Errors really should go to stderr not stdout.
    let error_string = stdout;
    assert!(!status.success());
    assert!(error_string.contains("is not a Conserve archive"), stderr);
}


#[test]
fn blackbox_backup() {
    let testdir = make_tempdir();
    let arch_dir = testdir.path().join("a");
    let arch_dir_str = arch_dir.to_str().unwrap();

    // conserve init
    let (status, stdout, stderr) = run_conserve(&["init", &arch_dir_str]);
    assert!(status.success());
    assert!(stdout
        .starts_with("Created new archive"));
    assert_eq!(stderr, "");

    // New archive contains no versions.
    let (status, stdout, stderr) = run_conserve(&["list-versions", &arch_dir_str]);
    assert_eq!(stderr, "");
    assert_eq!(stdout, "");
    assert!(status.success());

    let src = TreeFixture::new();
    src.create_file("hello");

    let (status, _stdout, stderr) = run_conserve(
        &["backup", &arch_dir_str, src.root.to_str().unwrap()]);
    assert!(status.success());
    assert_eq!("", stderr);
    // TODO: Inspect the archive

    assert_success_and_output(&["list-versions", &arch_dir_str],
        "b0000\n", "");

    // TODO: List files in the archive.
    // TODO: Restore.
    // TODO: Validate.
    // TODO: Compare vs source tree.
}


fn make_tempdir() -> tempdir::TempDir {
    tempdir::TempDir::new("conserve_blackbox").unwrap()
}


fn assert_success_and_output(args: &[&str], expected_stdout: &str, expected_stderr: &str) {
    let (status, stdout, stderr) = run_conserve(args);
    assert!(status.success(), "command {:?} failed unexpected", args);
    assert_eq!(expected_stderr, stderr);
    assert_eq!(expected_stdout, stdout);
}


/// Run Conserve's binary and return a `process::Output` including its return code, stdout
/// and stderr text.
///
/// Returns a tuple of: status, stdout_string, stderr_string.
fn run_conserve(args: &[&str]) -> (process::ExitStatus, String, String) {
    let mut conserve_path = env::current_exe().unwrap().to_path_buf();
    conserve_path.pop();  // Remove name of test binary
    conserve_path.push("conserve");
    let output = process::Command::new(&conserve_path).args(args).output()
        .expect("Failed to run conserve");
    (output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned())
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
