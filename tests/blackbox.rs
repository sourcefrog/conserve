// Conserve backup system.
// Copyright 2016 Martin Pool.

/// Run conserve CLI as a subprocess and test it.


extern crate conserve_testsupport;
#[macro_use]
extern crate spectral;
extern crate tempdir;

use std::env;
use std::io::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::str;

use spectral::prelude::*;

use conserve_testsupport::TreeFixture;


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
            conserve list-source [options] <source>
            conserve ls [options] <archive>
            conserve restore [options] <archive> <destination>
            conserve versions [options] <archive>
            conserve --version
            conserve --help
        ");
    assert_that(&stderr).is_equal_to(&expected_err);
    assert_eq!("", stdout);
}

#[test]
fn blackbox_version() {
    assert_success_and_output(&["--version"],
        "0.3.0\n", "");
}


#[test]
fn blackbox_help() {
    assert_success_and_output(
        &["--help"],
        &strip_indents("
            Conserve: a robust backup tool.
            Copyright 2015, 2016 Martin Pool, GNU GPL v2.
            http://conserve.fyi/

            Usage:
                conserve init [options] <archive>
                conserve backup [options] <archive> <source>
                conserve list-source [options] <source>
                conserve ls [options] <archive>
                conserve restore [options] <archive> <destination>
                conserve versions [options] <archive>
                conserve --version
                conserve --help

            Options:
                --stats         Show statistics at completion.
                --no-progress   No progress bar.
            "),
        "");
}


#[test]
fn clean_error_on_non_archive() {
    // Try to backup into a directory that is not an archive.
    let testdir = make_tempdir();
    let not_archive_path_str = testdir.path().to_str().unwrap();
    let (status, stdout, _) = run_conserve(&["backup", &not_archive_path_str, "."]);
    // TODO: Errors really should go to stderr not stdout.
    let error_string = stdout;
    assert_that(&status).matches(|s| !s.success());
    assert_that(&error_string.as_str()).contains(&"Not a Conserve archive");
}


#[test]
fn blackbox_backup() {
    let testdir = make_tempdir();
    let arch_dir = testdir.path().join("a");
    let arch_dir_str = arch_dir.to_str().unwrap();

    // conserve init
    let (status, stdout, stderr) = run_conserve(&["init", &arch_dir_str]);
    assert!(status.success());
    assert_that(&stdout.as_str()).starts_with(&"Created new archive");
    assert_eq!(stderr, "");

    // New archive contains no versions.
    let (status, stdout, stderr) = run_conserve(&["versions", &arch_dir_str]);
    assert_eq!(stderr, "");
    assert_eq!(stdout, "");
    assert!(status.success());

    let src = TreeFixture::new();
    src.create_file("hello");
    src.create_dir("subdir");

    let (status, _stdout, stderr) = run_conserve(
        &["backup", &arch_dir_str, src.root.to_str().unwrap()]);
    assert_that(&stderr.as_str()).is_equal_to(&"");
    assert!(status.success());
    // TODO: Inspect the archive

    assert_success_and_output(&["versions", &arch_dir_str],
        "b0000\n", "");

    assert_success_and_output(&["ls", &arch_dir_str],
        "/\n/hello\n/subdir\n",
        "");

    // TODO: Factor out comparison to expected tree.
    let restore_dir = make_tempdir();
    let restore_dir_str = restore_dir.path().to_str().unwrap();
    let (status, _stdout, _stderr) = run_conserve(
        &["restore", &arch_dir_str, &restore_dir_str]);
    assert!(status.success());
    assert!(fs::metadata(restore_dir.path().join("subdir")).unwrap().is_dir());

    let restore_hello = restore_dir.path().join("hello");
    assert!(fs::metadata(&restore_hello).unwrap().is_file());
    let mut file_contents = String::new();
    fs::File::open(&restore_hello).unwrap().read_to_string(&mut file_contents).unwrap();
    assert_eq!(file_contents, "contents");

    // TODO: Validate.
    // TODO: Compare vs source tree.
    //
    //     $ conserve restore myarchive restoredir
    //     $ cat restoredir/afile
    //     strawberry
    //
    // For safety, you cannot restore to the same directory twice:
    //
    //     $ conserve -L restore myarchive restoredir
    //     error creating restore destination directory "restoredir": File exists
    //     [3]
    //
    // There is a `validate` command that checks that an archive is internally
    // consistent and well formatted.  Validation doesn't compare the contents
    // of the archive to any external source.  Validation is intended to catch
    // bugs in Conserve, underlying software, or hardware errors -- in the
    // absence of such problems it should never fail.
    //
    // Validate just exits silently and successfully unless problems are
    // detected.
    //
    //     $ conserve validate myarchive
    //
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

/// Find the conserve binary.
///
/// It might be in the same directory as the test (if run from tests/debug) or
/// in the parent, if the test happens to be run from tests/debug/deps.
///
/// See https://users.rust-lang.org/t/test-dependency-binary-no-longer-found-under-unqualified-name/8077.
fn find_conserve_binary() -> PathBuf {
    let mut search_dir = env::current_exe().unwrap().to_path_buf();
    for _ in 0..2 {
        search_dir.pop();
        let mut conserve_path = search_dir.clone();
        conserve_path.push("conserve");
        conserve_path.set_extension(std::env::consts::EXE_EXTENSION);
        if conserve_path.as_path().exists() {
            return conserve_path;
        }
    }
    panic!("Can't find conserve binary under {:?}", search_dir);
}

/// Run Conserve's binary and return a `process::Output` including its return code, stdout
/// and stderr text.
///
/// Returns a tuple of: status, stdout_string, stderr_string.
fn run_conserve(args: &[&str]) -> (process::ExitStatus, String, String) {
    let conserve_path = find_conserve_binary();
    println!("run conserve: {:?}", args);
    let output = process::Command::new(&conserve_path).args(args).output()
        .expect(format!("Failed to run conserve: {:?} {:?}", &conserve_path, &args).as_str());
    println!("status: {:?}", output.status);
    let output_string = String::from_utf8_lossy(&output.stdout).into_owned();
    let error_string = String::from_utf8_lossy(&output.stderr).into_owned();
    println!(">> stdout:\n{}\n>> stderr:\n{}", &output_string, &error_string);
    (output.status, output_string, error_string)
}
