// Conserve backup system.
// Copyright 2016-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use indoc::indoc;
use predicates::prelude::*;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};

use crate::run_conserve;

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
        .args(["backup", "--exclude", "**/target"])
        .arg(arch_dir)
        .arg(src.path())
        .assert()
        .success();
}

#[test]
fn exclude_simple_glob() {
    let af = ScratchArchive::new();
    let src = TreeFixture::new();

    src.create_dir("src");
    src.create_file("src/hello.c");
    src.create_file("src/hello.o");

    run_conserve()
        .args(["backup", "-v", "--exclude", "*.o", "--no-stats"])
        .arg(af.path())
        .arg(src.path())
        .assert()
        .stdout("+ /src/hello.c\n")
        .success();

    run_conserve()
        .args(["ls"])
        .arg(af.path())
        .assert()
        .stdout("/\n/src\n/src/hello.c\n")
        .success();
}

/// `--exclude /*.o` should match only in the root directory.
#[test]
fn exclude_glob_only_in_root() {
    let af = ScratchArchive::new();
    let src = TreeFixture::new();

    src.create_dir("src");
    src.create_file("src/hello.c");
    src.create_file("src/hello.o");

    run_conserve()
        .args(["backup", "-v", "--exclude", "/*.o", "--no-stats"])
        .arg(af.path())
        .arg(src.path())
        .assert()
        .stdout("+ /src/hello.c\n+ /src/hello.o\n")
        .success();

    run_conserve()
        .args(["ls"])
        .arg(af.path())
        .assert()
        .stdout("/\n/src\n/src/hello.c\n/src/hello.o\n")
        .success();
}

#[test]
fn exclude_suffix_pattern() {
    let af = ScratchArchive::new();
    let src = TreeFixture::new();

    src.create_dir("src");
    src.create_file("src/hello.rs");
    src.create_dir("target");
    src.create_dir("target/release");
    src.create_dir("target/debug");
    src.create_dir("release");
    src.create_dir("subproj");
    src.create_dir("subproj/target");
    src.create_dir("subproj/target/release");

    run_conserve()
        .args(["backup", "-v", "--exclude", "target/{release,debug}"])
        .arg(af.path())
        .arg(src.path())
        .assert()
        .success();

    run_conserve()
        .args(["ls"])
        .arg(af.path())
        .assert()
        .stdout("/\n/release\n/src\n/subproj\n/target\n/src/hello.rs\n/subproj/target\n")
        .success();
}

#[test]
fn exclude_from_file() {
    let af = ScratchArchive::new();
    let src = TreeFixture::new();

    src.create_dir("src");
    src.create_file("src/hello.rs");
    src.create_dir("junk.tmp");
    src.create_dir("target");
    src.create_file("thing~");
    src.create_file_with_contents(
        "exclude",
        b"#some exclusions\n  *.tmp \n# hello.rs\n\n/target\n",
    );

    run_conserve()
        .args(["backup", "-v", "--exclude-from"])
        .arg(src.path().join("exclude"))
        .arg(af.path())
        .args(["--exclude=*~"])
        .arg(src.path())
        .assert()
        .success();
}

/// `--exclude /subtree` should also exclude everything under it.
///
/// <https://github.com/sourcefrog/conserve/issues/160>
#[test]
fn ls_exclude_excludes_subtrees() {
    run_conserve()
        .args([
            "ls",
            "--exclude",
            "/subdir",
            "testdata/archive/simple/v0.6.10",
        ])
        .assert()
        .success()
        .stdout(indoc! { "
            /
            /hello
        "})
        .stderr("");
}

/// `--exclude /subtree` should also exclude everything under it.
///
/// <https://github.com/sourcefrog/conserve/issues/160>
#[test]
fn restore_exclude_excludes_subtrees() {
    let dest = TempDir::new().unwrap();
    run_conserve()
        .args([
            "restore",
            "-v",
            "--no-stats",
            "--exclude",
            "/subdir",
            "testdata/archive/simple/v0.6.10",
        ])
        .arg(dest.path())
        .assert()
        .success()
        .stdout(indoc! { "
            + /
            + /hello
        "})
        .stderr("");
    dest.child("subdir").assert(predicate::path::missing());
}
