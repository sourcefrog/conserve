// Mostly inactive on Unix, as the mount function is not implemented for Unix.
#![cfg_attr(not(windows), allow(unused))]

use std::{
    fs::{self},
    path::Path,
};

use conserve::{
    BackupOptions, MountOptions, backup, monitor::test::TestMonitor, test_fixtures::TreeFixture,
};
use tempfile::TempDir;

#[cfg(windows)]
fn read_dir(path: &Path) -> Vec<(bool, String)> {
    fs::read_dir(path)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            (
                entry.file_type().unwrap().is_dir(),
                entry.file_name().to_string_lossy().to_string(),
            )
        })
        .collect::<Vec<_>>()
}

#[tokio::test]
#[cfg(unix)]
async fn mount_unix_not_implemented() {
    use assert_matches::assert_matches;
    use conserve::{Archive, Error};

    let archive = Archive::create_temp().await;
    let mountdir = TempDir::new().unwrap();

    let result = conserve::mount(
        archive.clone(),
        mountdir.path(),
        MountOptions { clean: false },
    );
    assert_matches!(result.err(), Some(Error::NotImplemented));
}

#[tokio::test]
#[cfg(not(unix))]
async fn mount_empty() {
    let archive = Archive::create_temp().await;
    let mountdir = TempDir::new().unwrap();
    let _projection = conserve::mount(
        archive.clone(),
        mountdir.path(),
        MountOptions { clean: false },
    )
    .unwrap();

    assert!(mountdir.path().is_dir());

    /* An empty projection should not contain the "latest" folder as there is no latest */
    assert_eq!(read_dir(mountdir.path()), [(true, "all".into())]);
}

#[tokio::test]
#[cfg(not(unix))]
async fn mount_sub_dirs() {
    let archive = Archive::create_temp().await;
    {
        let srcdir = TreeFixture::new();

        srcdir.create_dir("sub1");
        srcdir.create_dir("sub1/sub1");
        srcdir.create_file("sub1/sub1/file.txt");

        srcdir.create_dir("sub2");
        backup(
            &archive,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .unwrap();
    }

    let mountdir = TempDir::new().unwrap();
    let _projection = conserve::mount(
        archive.clone(),
        mountdir.path(),
        MountOptions { clean: false },
    )
    .unwrap();

    assert!(mountdir.path().is_dir());
    assert_eq!(
        read_dir(&mountdir.path().join("all")),
        [(true, "b0000".into())]
    );
    assert_eq!(
        read_dir(&mountdir.path().join("all").join("b0000")),
        [(true, "sub1".into()), (true, "sub2".into())]
    );
    assert_eq!(
        read_dir(&mountdir.path().join("all").join("b0000").join("sub1")),
        [(true, "sub1".into())]
    );
    assert_eq!(
        read_dir(
            &mountdir
                .path()
                .join("all")
                .join("b0000")
                .join("sub1")
                .join("sub1")
        ),
        [(false, "file.txt".into())]
    );
    assert_eq!(
        read_dir(&mountdir.path().join("all").join("b0000").join("sub2")),
        []
    );
}

#[tokio::test]
#[cfg(not(unix))]
async fn mount_file_versions() {
    let archive = Archive::create_temp().await;
    {
        let srcdir = TreeFixture::new();

        srcdir.create_file_with_contents("file_v1.txt", b"Hello World");
        backup(
            &archive,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .unwrap();

        srcdir.create_file_with_contents("file_v1.txt", b"Good bye World");
        srcdir.create_file_with_contents("file_v2.txt", b"Only in V2");
        backup(
            &archive,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .unwrap();
    }

    let mountdir = TempDir::new().unwrap();
    let _projection = conserve::mount(
        archive.clone(),
        mountdir.path(),
        MountOptions { clean: false },
    )
    .unwrap();

    assert!(mountdir.path().is_dir());
    assert_eq!(
        read_dir(mountdir.path()),
        [(true, "all".into()), (true, "latest".into())]
    );

    /* check that "latest" is actually the latest version (version 2) */
    assert_eq!(
        read_dir(&mountdir.path().join("latest")),
        [(false, "file_v1.txt".into()), (false, "file_v2.txt".into())]
    );
    assert_eq!(
        fs::read(mountdir.path().join("latest").join("file_v1.txt")).unwrap(),
        b"Good bye World"
    );

    /* check if the versions can be properly listed and accessed by "all" */
    assert_eq!(
        read_dir(&mountdir.path().join("all")),
        [(true, "b0000".into()), (true, "b0001".into())]
    );

    assert_eq!(
        read_dir(&mountdir.path().join("all").join("b0000")),
        [(false, "file_v1.txt".into())]
    );
    assert_eq!(
        fs::read(
            mountdir
                .path()
                .join("all")
                .join("b0000")
                .join("file_v1.txt")
        )
        .unwrap(),
        b"Hello World"
    );

    assert_eq!(
        read_dir(&mountdir.path().join("all").join("b0001")),
        [(false, "file_v1.txt".into()), (false, "file_v2.txt".into())]
    );
    assert_eq!(
        fs::read(
            mountdir
                .path()
                .join("all")
                .join("b0001")
                .join("file_v1.txt")
        )
        .unwrap(),
        b"Good bye World"
    );
}

#[tokio::test]
#[cfg(not(unix))]
async fn mount_cleanup() {
    let archive = Archive::create_temp().await;
    {
        let srcdir = TreeFixture::new();
        srcdir.create_file("file.txt");

        srcdir.create_dir("sub1");
        srcdir.create_file("sub1/file.txt");

        srcdir.create_dir("sub2");
        backup(
            &archive,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .unwrap();
    }

    let mountdir = TempDir::new().unwrap();
    fs::remove_dir(mountdir.path()).unwrap();

    let projection = conserve::mount(
        archive.clone(),
        mountdir.path(),
        MountOptions { clean: true },
    )
    .unwrap();

    assert!(mountdir.path().is_dir());

    /* actually read some data which may create files in the mount dir */
    fs::read(mountdir.path().join("all").join("b0000").join("file.txt")).unwrap();
    fs::read(
        mountdir
            .path()
            .join("all")
            .join("b0000")
            .join("sub1")
            .join("file.txt"),
    )
    .unwrap();
    assert!(!read_dir(mountdir.path()).is_empty());

    /* Mount dir should be cleaned now */
    drop(projection);

    /* the target dir should have been deleted */
    assert!(!mountdir.path().is_dir());
}
