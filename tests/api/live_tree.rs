// Copyright 2021, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use pretty_assertions::assert_eq;
use regex::Regex;

use conserve::test_fixtures::TreeFixture;
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
    let result: Vec<LiveEntry> = lt
        .iter_entries(Apath::root(), Exclude::nothing())
        .unwrap()
        .collect();
    let names = entry_iter_to_apath_strings(result.clone());
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

    let re_str = r#"LiveEntry \{ apath: Apath\("/jam/apricot"\), kind: "#.to_owned()
        + r#"File, mtime: UnixTime \{ [^)]* \}, size: Some\(8\), symlink_target: None, "#
        + r#"unix_mode: UnixMode\((Some\([0-9]+\)\)|None), "#
        + r#"owner: Owner \{ user: (Some\("[a-z_][a-z0-9_-]*[$]?"\)|None), "#
        + r#"group: (Some\("[a-z_][a-z0-9_-]*[$]?"\)|None) \} \}"#;

    let re = Regex::new(&re_str).unwrap();
    assert!(re.is_match(&repr));

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

    let exclude = Exclude::from_strings(["/**/fooo*", "/**/ba[pqr]", "/**/*bas"]).unwrap();

    let lt = LiveTree::open(tf.path()).unwrap();
    let names = entry_iter_to_apath_strings(lt.iter_entries(Apath::root(), exclude).unwrap());

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
    let names =
        entry_iter_to_apath_strings(lt.iter_entries(Apath::root(), Exclude::nothing()).unwrap());

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
        lt.iter_entries("/subdir".into(), Exclude::nothing())
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
    let names =
        entry_iter_to_apath_strings(lt.iter_entries(Apath::root(), Exclude::nothing()).unwrap());
    assert_eq!(names, ["/", "/a"]);
}

/// Collect apaths from an iterator into a list of string.
///
/// This is more loosely typed but useful for tests.
fn entry_iter_to_apath_strings<EntryIter, E>(entry_iter: EntryIter) -> Vec<String>
where
    EntryIter: IntoIterator<Item = E>,
    E: Entry,
{
    entry_iter
        .into_iter()
        .map(|entry| entry.apath().clone().into())
        .collect()
}
