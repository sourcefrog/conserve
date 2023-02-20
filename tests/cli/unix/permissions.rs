// Copyright 2023 Martin Pool
// Copyright 2022 Stephanie Aelmore

//! Tests for Unix permissions, run only on Unix.

use std::fs::set_permissions;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use indoc::{formatdoc, indoc};
use predicates::prelude::*;

use crate::run_conserve;

#[test]
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
        .stderr(predicate::str::is_empty());

    // copy the appropriate testdata into the testdir
    let src: PathBuf = "./testdata/tree/minimal".into();
    assert!(src.is_dir());

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

    // Find out which user and group is on the temporary directory.
    let mdata = std::fs::metadata(&data_dir).expect("Unable to read file metadata");
    dbg!(&mdata);
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
    let expected = format!(
        indoc! {"
                + r--r--r-- {user:<10} {group:<10} /hello
            "},
        //  + rwxr-xr-x {user:<10} {group:<10} /subdir/subfile
        //  Backup complete.
        user = user,
        group = group
    );
    println!("expected: {}", expected);
    run_conserve()
        .args(["backup", "-v", "-l"])
        .arg(&arch_dir)
        .arg(&data_dir)
        .assert()
        .success()
        .stderr(predicate::str::contains("Backup complete."))
        .stdout(predicate::str::starts_with(expected));

    // verify file permissions in stored archive
    run_conserve()
        .args(["ls", "-l"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::diff(formatdoc! { "
             rwxr-xr-x {user:<10} {group:<10} /
             r--r--r-- {user:<10} {group:<10} /hello
             rwxrwxr-x {user:<10} {group:<10} /subdir
             rwxr-xr-x {user:<10} {group:<10} /subdir/subfile
        " }));

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
        .stdout(predicate::str::diff(formatdoc! {"
             + rwxr-xr-x {user:<10} {group:<10} /
             + r--r--r-- {user:<10} {group:<10} /hello
             + rwxrwxr-x {user:<10} {group:<10} /subdir
             + rwxr-xr-x {user:<10} {group:<10} /subdir/subfile
        "}));
}

#[test]
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
        .stderr(predicate::str::is_empty());

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
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Backup complete.\n"));

    let restore_dir = TempDir::new().unwrap();

    // restore
    run_conserve()
        .args(["restore", "-v", "-l", "--no-progress", "--no-stats"])
        .arg(&arch_dir)
        .arg(restore_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::diff(formatdoc!(
            "
            + {} {} /
            + {} {} /hello
            + {} {} /subdir
            + {} {} /subdir/subfile
            ",
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
/// List an archive with particular encoded permissions, from the first version tha tracked
/// ownership and permissions.
///
/// This should succeed even, and especially, if the machine running the tests does
/// not have users/groups matching those in the archive.
fn list_testdata_with_permissions() {
    let archive_path = Path::new("testdata/archive/minimal/v0.6.17");
    run_conserve()
        .args(["ls", "-l"])
        .arg(archive_path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::diff(
            "\
            rwxrwxr-x mbp        mbp        /\n\
            rw-rw-r-- mbp        mbp        /hello\n\
            rwxrwxr-x mbp        mbp        /subdir\n\
            rw-rw-r-- mbp        mbp        /subdir/subfile\n\
            ",
        ));
}
