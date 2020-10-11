// Conserve backup system.
// Copyright 2016, 2017, 2018, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Run conserve CLI as a subprocess and test it.

use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use escargot::CargoRun;
use lazy_static::lazy_static;
use predicates::prelude::*;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};

lazy_static! {
    // This doesn's pass `.current_target()` because it doesn't seem
    // necessary for typical cases (cross-builds won't work with this)
    // and it causes everything to rebuild which slows the tests a lot.
    static ref CARGO_RUN: CargoRun = escargot::CargoBuild::new()
        .current_release()
        .run() // Build it and return a proxy to run it
        .unwrap();
}

fn run_conserve() -> Command {
    CARGO_RUN.command()
}

#[test]
fn no_args() {
    // Run with no arguments, should fail with a usage message to stderr.
    run_conserve()
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("USAGE:"));
}

#[test]
fn help() {
    run_conserve()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("A robust backup tool"))
        .stdout(predicate::str::contains(
            "Copy source directory into an archive",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn clean_error_on_non_archive() {
    // Try to backup into a directory that is not an archive.
    let testdir = TempDir::new().unwrap();
    // TODO: Errors really should go to stderr not stdout.
    run_conserve()
        .arg("backup")
        .arg(testdir.path())
        .arg(".")
        .assert()
        .failure()
        .stdout(predicate::str::contains("Not a Conserve archive"));
}

#[test]
fn basic_backup() {
    let testdir = TempDir::new().unwrap();
    let arch_dir = testdir.path().join("a");

    // conserve init
    run_conserve()
        .arg("init")
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with("Created new archive"));

    // New archive contains no versions.
    run_conserve()
        .arg("versions")
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::is_empty());

    let src: PathBuf = "./testdata/tree/minimal".into();
    assert!(src.is_dir());

    run_conserve()
        .args(&["ls", "--source"])
        .arg(&src)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(
            "/\n\
             /hello\n\
             /subdir\n\
             /subdir/subfile\n",
        );

    run_conserve()
        .args(&["size", "-s"])
        .arg(&src)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("0 MB\n"); // "contents"

    // backup
    run_conserve()
        .arg("backup")
        .arg(&arch_dir)
        .arg(&src)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with("Backup complete.\n"));
    // TODO: Now inspect the archive.

    run_conserve()
        .args(&["size"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("0 MB\n"); // "contents"

    run_conserve()
        .arg("diff")
        .arg(&arch_dir)
        .arg(&src)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(
            "\
both     /
both     /hello
both     /subdir
both     /subdir/subfile
",
        );

    run_conserve()
        .args(&["versions", "--short"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("b0000\n");

    let expected_blocks = [
        "1e99127adff52dec50072705c860e753b2d9c14c0e019bf9a258071699aac38db7d604b3e4ac5345d81ec7e3d8810a805a4e5ff3a44a9f7aa94d120220d2873a",
        "fec91c70284c72d0d4e3684788a90de9338a5b2f47f01fedbe203cafd68708718ae5672d10eca804a8121904047d40d1d6cf11e7a76419357a9469af41f22d01",
    ];
    let is_expected_blocks = |output: &[u8]| {
        let output_str = std::str::from_utf8(&output).unwrap();
        let mut blocks: Vec<&str> = output_str.lines().collect();
        blocks.sort();
        blocks == expected_blocks
    };

    run_conserve()
        .args(&["debug", "blocks"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::function(is_expected_blocks));

    run_conserve()
        .args(&["debug", "referenced"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::function(is_expected_blocks));

    run_conserve()
        .args(&["debug", "unreferenced"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr("")
        .stdout("");

    run_conserve()
        .args(&["debug", "index"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    // TODO: Deserialize index json, or somehow check it.

    // gc: should find no garbage.
    run_conserve().arg("gc").arg(&arch_dir).assert().success();

    run_conserve()
        .arg("versions")
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(
            predicate::str::is_match(
                r"^b0000 *complete   20\d\d-\d\d-\d\d \d\d:\d\d:\d\d +0:\d+\n$",
            )
            .unwrap(),
        );
    // TODO: Set a fake date when creating the archive and then we can check
    // the format of the output?

    run_conserve()
        .arg("versions")
        .arg("--sizes")
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(
            predicate::str::is_match(
                r"^b0000 *complete   20\d\d-\d\d-\d\d \d\d:\d\d:\d\d +0:\d+ *0 MB\n$",
            )
            .unwrap(),
        );

    run_conserve()
        .arg("ls")
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(
            "/\n\
             /hello\n\
             /subdir\n\
             /subdir/subfile\n",
        );

    // TODO: Factor out comparison to expected tree.
    let restore_dir = TempDir::new().unwrap();

    run_conserve()
        .arg("restore")
        .arg("-v")
        .arg(&arch_dir)
        .arg(restore_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with(
            "/\n\
             /hello\n\
             /subdir\n\
             /subdir/subfile\n\
             Restore complete.\n",
        ));

    restore_dir
        .child("subdir")
        .assert(predicate::path::is_dir());
    restore_dir
        .child("hello")
        .assert(predicate::path::is_file())
        .assert("hello world\n");
    restore_dir
        .child("subdir")
        .child("subfile")
        .assert("I like Rust\n");

    // Try to restore again over the same directory: should decline.
    run_conserve()
        .arg("restore")
        .arg("-v")
        .arg(&arch_dir)
        .arg(restore_dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("Destination directory not empty"));

    // Restore with specified band id / backup version.
    {
        let restore_dir2 = TempDir::new().unwrap();
        // Try to restore again over the same directory: should decline.
        run_conserve()
            .args(&["restore", "-b", "b0"])
            .arg(&arch_dir)
            .arg(restore_dir2.path())
            .assert()
            .success();
        // TODO: Check tree contents, but they should be the same as above.
    }

    // Validate
    run_conserve()
        .arg("validate")
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("Archive is OK.\n"));

    // TODO: Compare vs source tree.
}

#[test]
fn empty_archive() {
    let tempdir = TempDir::new().unwrap();
    let adir = tempdir.path().join("archive");
    let restore_dir = TempDir::new().unwrap();

    run_conserve().arg("init").arg(&adir).assert().success();

    run_conserve()
        .arg("restore")
        .arg(&adir)
        .arg(restore_dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("Archive has no bands"));

    run_conserve()
        .arg("ls")
        .arg(&adir)
        .assert()
        .failure()
        .stdout(predicate::str::contains("Archive has no bands"));

    run_conserve()
        .arg("versions")
        .arg(&adir)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    run_conserve().arg("gc").arg(adir).assert().success();
}

/// Check behavior on an incomplete version.
///
/// The `--incomplete` option is no longer needed.
#[test]
fn incomplete_version() {
    let af = ScratchArchive::new();
    af.setup_incomplete_empty_band();

    run_conserve()
        .arg("versions")
        .arg(af.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("b0000"))
        .stdout(predicate::str::contains("incomplete"));

    // ls succeeds on an incomplete band
    run_conserve().arg("ls").arg(af.path()).assert().success();

    // Cannot gc with an empty band.
    run_conserve()
        .arg("gc")
        .arg(af.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("incomplete and may be in use"));
}

#[test]
fn exclude_option_ordering() {
    // Regression caused by the move to structopt(?) in 7ddb02d0cf47467f1cccc2dcdedb005e8c4e3f25.
    // See https://github.com/TeXitoi/structopt/issues/396.
    let testdir = TempDir::new().unwrap();
    let arch_dir = testdir.path().join("a");

    // conserve init
    run_conserve().arg("init").arg(&arch_dir).assert().success();

    let src = TreeFixture::new();
    src.create_file("hello");
    src.create_dir("subdir");

    run_conserve()
        .args(&["backup", "--exclude", "**/target"])
        .arg(&arch_dir)
        .arg(&src.path())
        .assert()
        .success();
}

#[test]
fn validate_non_fatal_problems_nonzero_result() {
    run_conserve()
        .args(&["validate", "testdata/damaged/missing-block/"])
        .assert()
        .stdout(predicate::str::contains("Archive has some problems."))
        .code(2);
}

#[test]
fn restore_only_subtree() {
    let dest = TempDir::new().unwrap();
    run_conserve()
        .args(&[
            "restore",
            "testdata/archive/v0.6.3/minimal-1/",
            "--only",
            "/subdir",
        ])
        .arg(&dest.path())
        .assert()
        .success();

    dest.child("hello").assert(predicate::path::missing());
    dest.child("subdir").assert(predicate::path::is_dir());
    dest.child("subdir")
        .child("subfile")
        .assert("I like Rust\n");

    dest.close().unwrap();
}

#[test]
fn delete_bands() {
    let af = ScratchArchive::new();
    af.store_two_versions();

    run_conserve()
        .args(&["delete"])
        .args(&["-b", "b0000"])
        .args(&["-b", "b0001"])
        .arg(af.path())
        .assert()
        .success();
}

#[test]
fn delete_nonexistent_band() {
    let af = ScratchArchive::new();

    let pred_fn = predicate::str::is_match(
        r"conserve error: Failed to delete band b0000
  caused by: (No such file or directory|The system cannot find the file specified\.) \(os error \d+\)
",
        )
        .unwrap();

    run_conserve()
        .args(&["delete"])
        .args(&["-b", "b0000"])
        .arg(af.path())
        .assert()
        .stdout(pred_fn)
        .failure();
}

#[test]
fn size_exclude() {
    let source = TreeFixture::new();
    source.create_file_with_contents("small", b"0123456789");
    source.create_file_with_contents("junk", b"01234567890123456789");

    run_conserve()
        .args(&["size", "--bytes", "--source"])
        .arg(&source.path())
        .args(&["--exclude=/junk"])
        .assert()
        .success()
        .stdout("10\n");
}
