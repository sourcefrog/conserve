// Copyright 2021-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use conserve::monitor::test::TestMonitor;
use pretty_assertions::assert_eq;

use conserve::entry::EntryValue;
use conserve::test_fixtures::{entry_iter_to_apath_strings, TreeFixture};
use conserve::*;

#[test]
fn open_tree() {
    let tf = TreeFixture::new();
    let lt = LiveTree::open(tf.path()).unwrap();
    assert_eq!(lt.path(), tf.path());
}

#[test]
fn list_simple_directory() {
    let tf = TreeFixture::new();
    tf.create_file("bba");
    tf.create_file("aaa");
    tf.create_dir("jam");
    tf.create_file("jam/apricot");
    tf.create_dir("jelly");
    tf.create_dir("jam/.etc");
    let lt = LiveTree::open(tf.path()).unwrap();
    let result: Vec<EntryValue> = lt
        .iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
        .unwrap()
        .collect();
    let names = entry_iter_to_apath_strings(&result);
    // First one is the root
    assert_eq!(
        names,
        [
            "/",
            "/aaa",
            "/bba",
            "/jam",
            "/jelly",
            "/jam/.etc",
            "/jam/apricot"
        ]
    );

    let repr = format!("{:?}", &result[6]);
    println!("{repr}");
    assert!(repr.starts_with("EntryValue {"));
    assert!(repr.contains("Apath(\"/jam/apricot\")"));

    // TODO: Somehow get the stats out of the iterator.
    // assert_eq!(source_iter.stats.directories_visited, 4);
    // assert_eq!(source_iter.stats.entries_returned, 7);
}

#[test]
fn exclude_entries_directory() {
    let tf = TreeFixture::new();
    tf.create_file("foooo");
    tf.create_file("bar");
    tf.create_dir("fooooBar");
    tf.create_dir("baz");
    tf.create_file("baz/bar");
    tf.create_file("baz/bas");
    tf.create_file("baz/test");

    let exclude = Exclude::from_strings(["/**/fooo*", "/**/??[rs]", "/**/*bas"]).unwrap();

    let lt = LiveTree::open(tf.path()).unwrap();
    let names = entry_iter_to_apath_strings(
        lt.iter_entries(Apath::root(), exclude, TestMonitor::arc())
            .unwrap(),
    );

    // First one is the root
    assert_eq!(names, ["/", "/baz", "/baz/test"]);

    // TODO: Get stats back from the iterator
    // assert_eq!(source_iter.stats.directories_visited, 2);
    // assert_eq!(source_iter.stats.entries_returned, 3);
    // assert_eq!(source_iter.stats.exclusions, 5);
}

#[cfg(unix)]
#[test]
fn symlinks() {
    let tf = TreeFixture::new();
    tf.create_symlink("from", "to");

    let lt = LiveTree::open(tf.path()).unwrap();
    let names = entry_iter_to_apath_strings(
        lt.iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
            .unwrap(),
    );

    assert_eq!(names, ["/", "/from"]);
}

#[test]
fn iter_subtree_entries() {
    let tf = TreeFixture::new();
    tf.create_file("in base");
    tf.create_dir("subdir");
    tf.create_file("subdir/a");
    tf.create_file("subdir/b");
    tf.create_file("zzz");

    let lt = LiveTree::open(tf.path()).unwrap();

    let names = entry_iter_to_apath_strings(
        lt.iter_entries("/subdir".into(), Exclude::nothing(), TestMonitor::arc())
            .unwrap(),
    );
    assert_eq!(names, ["/subdir", "/subdir/a", "/subdir/b"]);
}

#[test]
fn exclude_cachedir() {
    let tf = TreeFixture::new();
    tf.create_file("a");
    let cache_dir = tf.create_dir("cache");
    tf.create_dir("cache/1");
    cachedir::add_tag(cache_dir).unwrap();

    let lt = LiveTree::open(tf.path()).unwrap();
    let names = entry_iter_to_apath_strings(
        lt.iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
            .unwrap(),
    );
    assert_eq!(names, ["/", "/a"]);
}
