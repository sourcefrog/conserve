// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Find source files within a source directory, in apath order.

use std::cmp::Ordering;
use std::collections::vec_deque::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::apath;
use super::report::Report;


/// An entry found in the source directory.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Conserve apath, relative to the top-level directory.
    pub apath: String,

    /// Possibly absolute path through which the file can be opened.
    pub path: PathBuf,
}


/// Recursive iterator of the contents of a source directory.
#[derive(Debug)]
pub struct Iter {
    /// Directories yet to be visited.
    dir_deque: VecDeque<Entry>,

    /// Direct children of the current directory yet to be returned.
    entry_deque: VecDeque<Entry>,

    /// Count of directories and files visited by this iterator.
    report: Report,

    /// Copy of the last-emitted apath, for the purposes of checking they're in apath order.
    last_apath: Option<String>,
}


impl Iter {
    pub fn get_report(self: &Iter) -> Report {
        self.report.clone()
    }

    fn unchecked_next(&mut self) -> Option<io::Result<Entry>> {
        loop {
            if let Some(entry) = self.entry_deque.pop_front() {
                // Have already found some entries and just need to return them.
                self.report.increment("source.selected.count", 1);
                return Some(Ok(entry));
            } else if let Some(entry) = self.dir_deque.pop_front() {
                if let Err(e) = self.visit_next_directory(entry) {
                    return Some(Err(e));
                }
                // Queues have been refilled.
            } else {
                // No entries queued and no more directories to visit.
                return None;
            }
        }
    }

    fn visit_next_directory(&mut self, dir_entry: Entry) -> io::Result<()> {
        let readdir = try!(fs::read_dir(&dir_entry.path));
        self.report.increment("source.visited.directories.count", 1);
        let mut children = Vec::<(OsString, bool)>::new();
        for entry in readdir {
            let entry = try!(entry);
            let ft = try!(entry.file_type());
            children.push((entry.file_name(), ft.is_dir()));
        };
        children.sort();
        let mut directory_insert_point = 0;
        for (child_name, is_dir) in children {
            let mut new_apath = dir_entry.apath.to_string();
            if new_apath != "/" {
                new_apath.push('/');
            }
            new_apath.push_str(&child_name.to_string_lossy());
            let new_entry = Entry {
                apath: new_apath,
                path: dir_entry.path.join(child_name).to_path_buf(),
            };
            if is_dir {
                self.dir_deque.insert(directory_insert_point, new_entry.clone());
                directory_insert_point += 1;
            }
            self.entry_deque.push_back(new_entry);
        };
        Ok(())
    }
}


// The source iterator yields one path at a time as it walks through the source directories.
//
// It has to read each directory entirely so that it can sort the entries.
// These entries are then returned before visiting any subdirectories.
//
// It also has to manage a stack of directories which might be partially walked.  Those
// subdirectories are then visited, also in sorted order, before returning to
// any higher-level directories.
impl Iterator for Iter {
    type Item = io::Result<Entry>;

    fn next(&mut self) -> Option<io::Result<Entry>> {
        // Check that all the returned paths are in correct order.
        // TODO: Maybe this can be skipped in non-debug builds?
        match self.unchecked_next() {
            None => None,
            e @ Some(Err(_)) => e,
            Some(Ok(entry)) => {
                if let Some(ref last_apath) = self.last_apath {
                    assert_eq!(
                        apath::cmp(last_apath, &entry.apath),
                        Ordering::Less,
                        "sources returned out of order: {} >= {}",
                        last_apath, entry.apath);
                }
                self.last_apath = Some(entry.apath.clone());
                Some(Ok(entry))
            },
        }
    }
}

/// Iterate source files descending through a source directory.
///
/// Visit the files in a directory before descending into its children, as
/// is the defined order for files stored in an archive.  Within those files and
/// child directories, visit them according to a sorted comparison by their UTF-8
/// name.
///
/// The `Iter` has its own `Report` of how many directories and files were visited.
pub fn iter(source_dir: &Path) -> Iter {
    let root_entry = Entry {
        apath: "/".to_string(),
        path: source_dir.to_path_buf(),
    };
    // Preload iter to return the root and then recurse into it.
    let mut entry_deque: VecDeque<Entry> = VecDeque::<Entry>::new();
    entry_deque.push_back(root_entry.clone());
    let mut dir_deque = VecDeque::<Entry>::new();
    dir_deque.push_back(root_entry);
    Iter {
        entry_deque: entry_deque,
        dir_deque: dir_deque,
        report: Report::new(),
        last_apath: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::itertools;
    use super::super::testfixtures::TreeFixture;
    use super::super::report::Report;

    #[test]
    fn simple_directory() {
        let tf = TreeFixture::new();
        tf.create_file("bba");
        tf.create_file("aaa");
        tf.create_dir("jam");
        tf.create_file("jam/apricot");
        tf.create_dir("jelly");
        tf.create_dir("jam/.etc");
        let mut source_iter = iter(tf.path());
        let result = itertools::result_iter_to_vec(&mut source_iter).unwrap();
        // First one is the root
        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[0].path, &tf.root);
        assert_eq!(&result[1].apath, "/aaa");
        assert_eq!(&result[1].path, &tf.root.join("aaa"));
        assert_eq!(&result[2].apath, "/bba");
        assert_eq!(&result[2].path, &tf.root.join("bba"));
        assert_eq!(&result[3].apath, "/jam");
        assert_eq!(&result[3].path, &tf.root.join("jam"));
        assert_eq!(&result[4].apath, "/jelly");
        assert_eq!(&result[4].path, &tf.root.join("jelly"));
        assert_eq!(&result[5].apath, "/jam/.etc");
        assert_eq!(&result[5].path, &tf.root.join("jam").join(".etc"));
        assert_eq!(&result[6].apath, "/jam/apricot");
        assert_eq!(&result[6].path, &tf.root.join("jam").join("apricot"));
        assert_eq!(result.len(), 7);

        let report = source_iter.get_report();
        assert_eq!(report.get_count("source.visited.directories.count"), 4);
        assert_eq!(report.get_count("source.selected.count"), 7);
    }
}
