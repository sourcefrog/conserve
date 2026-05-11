// Copyright 2023 Martin Pool
// Copyright 2022 Stephanie Aelmore

//! Tests for Unix permissions, run only on Unix.

use std::fs::set_permissions;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use assert_cmd::prelude::*;
use assert_fs::TempDir;
use assert_fs::prelude::*;
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
    let user = uzers::get_user_by_uid(mdata.uid())
        .expect("Unable to find user by uid")
        .name()
        .to_str()
        .unwrap()
        .to_string();
    let group = uzers::get_group_by_gid(mdata.gid())
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
    println!("expected: {expected}");
    run_conserve()
        .args(["backup", "-v", "-l"])
        .arg(&arch_dir)
        .arg(&data_dir)
        .assert()
        .success()
        .stderr(predicate::str::contains("Backup complete."))
        .stdout(predicate::str::starts_with(expected));

    // verify file permissions in stored archive
    // Now includes mtime, so we check that each line contains the expected parts
    let output = run_conserve()
        .args(["ls", "-l"])
        .arg(&arch_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains(&format!("rwxr-xr-x {user:<10} {group:<10}")), "Missing / entry");
    assert!(stdout.contains(&format!("r--r--r-- {user:<10} {group:<10}")), "Missing /hello entry");
    assert!(stdout.contains(&format!("rwxrwxr-x {user:<10} {group:<10}")), "Missing /subdir entry");
    assert!(stdout.contains(&format!("rwxr-xr-x {user:<10} {group:<10}")), "Missing /subdir/subfile entry");

    // create a directory to restore to
    let restore_dir = TempDir::new().unwrap();

    // verify permissions are restored correctly
    // Now includes mtime in verbose restore output too
    let restore_output = run_conserve()
        .args(["restore", "-v", "-l", "--no-stats"])
        .arg(&arch_dir)
        .arg(&*restore_dir)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let restore_stdout = String::from_utf8_lossy(&restore_output);
    assert!(restore_stdout.contains(&format!("+ rwxr-xr-x {user:<10} {group:<10}")), "Missing restored /");
    assert!(restore_stdout.contains(&format!("+ r--r--r-- {user:<10} {group:<10}")), "Missing restored /hello");
    assert!(restore_stdout.contains(&format!("+ rwxrwxr-x {user:<10} {group:<10}")), "Missing restored /subdir");
    assert!(restore_stdout.contains(&format!("+ rwxr-xr-x {user:<10} {group:<10}")), "Missing restored /subdir/subfile");
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

    // verify ls command (now includes mtime, so check parts)
    let ls_output = run_conserve()
        .args(["ls", "-l", "--source"])
        .arg(&src)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let ls_stdout = String::from_utf8_lossy(&ls_output);
    // Check that it contains the mode and owner parts (mtime will vary)
    for line in expected.lines() {
        if !line.is_empty() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                // Check mode and owner are present
                assert!(ls_stdout.contains(parts[0]), "Missing mode {}", parts[0]);
                assert!(ls_stdout.contains(parts[1]), "Missing owner {}", parts[1]);
            }
        }
    }

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

    // restore (now includes mtime in output)
    let restore_output = run_conserve()
        .args(["restore", "-v", "-l", "--no-progress", "--no-stats"])
        .arg(&arch_dir)
        .arg(restore_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let restore_stdout = String::from_utf8_lossy(&restore_output);
    // Check that mode and owner are present in output (mtime will vary)
    assert!(restore_stdout.contains(&format!("+ {} {}", UnixMode::from(mdata_root.permissions()), Owner::from(&mdata_root))));
    assert!(restore_stdout.contains(&format!("+ {} {}", UnixMode::from(mdata_hello.permissions()), Owner::from(&mdata_hello))));
    assert!(restore_stdout.contains(&format!("+ {} {}", UnixMode::from(mdata_subdir.permissions()), Owner::from(&mdata_subdir))));
    assert!(restore_stdout.contains(&format!("+ {} {}", UnixMode::from(mdata_subdir_subfile.permissions()), Owner::from(&mdata_subdir_subfile))));
    assert!(restore_stdout.contains("/hello"));
    assert!(restore_stdout.contains("/subdir"));
    assert!(restore_stdout.contains("/subdir/subfile"));

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
/// List an archive with particular encoded permissions, from the first version that tracked
/// ownership and permissions.
///
/// This should succeed even, and especially, if the machine running the tests does
/// not have users/groups matching those in the archive.
fn list_testdata_with_permissions() {
    let archive_path = Path::new("testdata/archive/minimal/v0.6.17");
    // Now includes mtime in output
    let output = run_conserve()
        .args(["ls", "-l"])
        .arg(archive_path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("rwxrwxr-x mbp        mbp"), "Missing / entry");
    assert!(stdout.contains("rw-rw-r-- mbp        mbp        "), "Missing /hello entry");
    assert!(stdout.contains("/hello"), "Missing /hello path");
    assert!(stdout.contains("/subdir"), "Missing /subdir path");
    assert!(stdout.contains("/subdir/subfile"), "Missing /subdir/subfile path");
}
