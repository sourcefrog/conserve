// Conserve backup system.
// Copyright 2016, 2017, 2018, 2019, 2020, 2021, 2022 Martin Pool.

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
use predicates::prelude::*;
use url::Url;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};

mod backup;
mod delete;
mod diff;
mod exclude;
mod versions;

fn run_conserve() -> Command {
    Command::cargo_bin("conserve").expect("locate conserve binary")
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
        .args(["ls", "--source"])
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
        .args(["size", "-s"])
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
        .args(["size"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("0 MB\n"); // "contents"

    run_conserve()
        .args(["versions", "--short"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("b0000\n");

    let expected_blocks = [
        "ea50e43840e5f310490bba1b641db82480a05e16e9ae220c1e5113c79b59541fa5a6ddb13db20d4df53dfcecb3ed9969e41a329e07afe0fbb597251a789c3575",
    ];
    let is_expected_blocks = |output: &[u8]| {
        let output_str = std::str::from_utf8(output).unwrap();
        let mut blocks: Vec<&str> = output_str.lines().collect();
        blocks.sort_unstable();
        blocks == expected_blocks
    };

    run_conserve()
        .args(["debug", "blocks"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::function(is_expected_blocks));

    run_conserve()
        .args(["debug", "referenced"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::function(is_expected_blocks));

    run_conserve()
        .args(["debug", "unreferenced"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr("")
        .stdout("");

    run_conserve()
        .args(["debug", "index"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    // TODO: Deserialize index json, or somehow check it.

    // gc: should find no garbage.
    run_conserve().arg("gc").arg(&arch_dir).assert().success();

    // You can open it with a file URL.
    let file_url = Url::from_directory_path(&arch_dir).unwrap();
    run_conserve()
        .arg("ls")
        .arg(file_url.as_str())
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

    // Also try --no-progress here; should make no difference because these tests run
    // without a pty.
    run_conserve()
        .arg("restore")
        .arg("-v")
        .arg("--no-progress")
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
            .args(["restore", "-b", "b0"])
            .arg(&arch_dir)
            .arg(restore_dir2.path())
            .assert()
            .success();
        // TODO: Check tree contents, but they should be the same as above.
    }

    // Validate
    run_conserve()
        .arg("validate")
        .arg(arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("Archive is OK.\n"));

    // TODO: Compare vs source tree.
}

#[test]
#[cfg(unix)]
fn backup_unix_permissions() {
    use std::fs::Permissions;

    let testdir = TempDir::new().unwrap();
    let arch_dir = testdir.path().join("a");
    let data_dir = testdir.path().join("data");

    // conserve init
    run_conserve()
        .arg("init")
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with("Created new archive"));

    // copy the appropriate testdata into the testdir
    let src: PathBuf = "./testdata/tree/minimal".into();
    assert!(src.is_dir());

    // imports for this test
    use std::fs::set_permissions;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    // set up test directory
    cp_r::CopyOptions::new()
        .copy_tree(&src, &data_dir)
        .expect("Failed to copy files into test dir");
    set_permissions(&data_dir, Permissions::from_mode(0o755)).unwrap();

    // set subdir as group-writable
    set_permissions(data_dir.join("subdir"), Permissions::from_mode(0o775))
        .expect("Error setting file permissions");
    // set subdir/subfile as executable
    set_permissions(
        data_dir.join("subdir").join("subfile"),
        Permissions::from_mode(0o755),
    )
    .expect("Error setting file permissions");
    // set hello as readonly
    set_permissions(data_dir.join("hello"), Permissions::from_mode(0o444))
        .expect("Error setting file permissions");

    let mdata = std::fs::metadata(&src).expect("Unable to read file metadata");
    let user = users::get_user_by_uid(mdata.uid())
        .expect("Unable to find user by uid")
        .name()
        .to_str()
        .unwrap()
        .to_string();
    let group = users::get_group_by_gid(mdata.gid())
        .expect("Unable to find user by uid")
        .name()
        .to_str()
        .unwrap()
        .to_string();

    // backup
    run_conserve()
        .args(["backup", "-v", "-l"])
        .arg(&arch_dir)
        .arg(&data_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with(format!(
            "+ r--r--r-- {user:<10} {group:<10} /hello\n\
             + rwxr-xr-x {user:<10} {group:<10} /subdir/subfile\n\
             Backup complete."
        )));

    // verify file permissions in stored archive
    run_conserve()
        .args(["ls", "-l"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::diff(format!(
            "rwxr-xr-x {user:<10} {group:<10} /\n\
             r--r--r-- {user:<10} {group:<10} /hello\n\
             rwxrwxr-x {user:<10} {group:<10} /subdir\n\
             rwxr-xr-x {user:<10} {group:<10} /subdir/subfile\n"
        )));

    // create a directory to restore to
    let restore_dir = TempDir::new().unwrap();

    // verify permissions are restored correctly
    run_conserve()
        .args(["restore", "-v", "-l"])
        .arg(&arch_dir)
        .arg(&*restore_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with(format!(
            "rwxr-xr-x {user:<10} {group:<10} /\n\
             r--r--r-- {user:<10} {group:<10} /hello\n\
             rwxrwxr-x {user:<10} {group:<10} /subdir\n\
             rwxr-xr-x {user:<10} {group:<10} /subdir/subfile\n\
             Restore complete.\n"
        )));
}

#[test]
#[cfg(unix)]
fn backup_user_and_permissions() {
    // TODO: rewrite this test to properly test user and group somehow

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

    let src: PathBuf = "./testdata/tree/minimal".into();
    assert!(src.is_dir());

    use conserve::owner::Owner;
    use conserve::unix_mode::UnixMode;

    let mut path = src.clone();

    let mdata_root = std::fs::metadata(&path).expect("Unable to read / metadata");
    let mut expected = format!(
        "{} {} /\n",
        UnixMode::from(mdata_root.permissions()),
        Owner::from(&mdata_root)
    );
    path.push("hello");
    let mdata_hello = std::fs::metadata(&path).expect("Unable to read /hello metadata");
    expected.push_str(&format!(
        "{} {} /hello\n",
        UnixMode::from(mdata_hello.permissions()),
        Owner::from(&mdata_hello)
    ));

    path.pop();
    path.push("subdir");
    let mdata_subdir = std::fs::metadata(&path).expect("Unable to read /subdir metadata");
    expected.push_str(&format!(
        "{} {} /subdir\n",
        UnixMode::from(mdata_subdir.permissions()),
        Owner::from(&mdata_subdir)
    ));

    path.push("subfile");
    let mdata_subdir_subfile =
        std::fs::metadata(&path).expect("Unable to read /subdir/subfile metadata");
    expected.push_str(&format!(
        "{} {} /subdir/subfile\n",
        UnixMode::from(mdata_subdir_subfile.permissions()),
        Owner::from(&mdata_subdir_subfile)
    ));

    // verify ls command
    run_conserve()
        .args(["ls", "-l", "--source"])
        .arg(&src)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(expected);

    // backup
    run_conserve()
        .args(["backup"])
        .arg(&arch_dir)
        .arg(&src)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with("Backup complete.\n"));

    let restore_dir = TempDir::new().unwrap();

    // restore
    run_conserve()
        .args(["restore", "-v", "-l", "--no-progress"])
        .arg(&arch_dir)
        .arg(restore_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with(format!(
            "{} {} /\n\
             {} {} /hello\n\
             {} {} /subdir\n\
             {} {} /subdir/subfile\n\
             Restore complete.\n",
            UnixMode::from(mdata_root.permissions()),
            Owner::from(&mdata_root),
            UnixMode::from(mdata_hello.permissions()),
            Owner::from(&mdata_hello),
            UnixMode::from(mdata_subdir.permissions()),
            Owner::from(&mdata_subdir),
            UnixMode::from(mdata_subdir_subfile.permissions()),
            Owner::from(&mdata_subdir_subfile)
        )));

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
fn validate_non_fatal_problems_nonzero_result() {
    run_conserve()
        .args(["validate", "testdata/damaged/missing-block/"])
        .assert()
        .stdout(predicate::str::contains("Archive has some problems."))
        .code(2);
}

#[test]
fn restore_only_subtree() {
    let dest = TempDir::new().unwrap();
    run_conserve()
        .args([
            "restore",
            "testdata/archive/minimal/v0.6.3/",
            "--only",
            "/subdir",
        ])
        .arg(dest.path())
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
fn size_exclude() {
    let source = TreeFixture::new();
    source.create_file_with_contents("small", b"0123456789");
    source.create_file_with_contents("junk", b"01234567890123456789");

    run_conserve()
        .args(["size", "--bytes", "--source"])
        .arg(source.path())
        .args(["--exclude=/junk"])
        .assert()
        .success()
        .stdout("10\n");
}
