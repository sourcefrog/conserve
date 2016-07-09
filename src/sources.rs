// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Find source files within a source directory, in apath order.

use std::collections::vec_deque::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};


/// An entry found in the source directory.
pub struct Entry {
    /// Conserve apath, relative to the top-level directory.
    pub apath: String,

    /// Possibly absolute path through which the file can be opened.
    pub path: PathBuf,
}


/// Recursive iterator of the contents of a source directory.
pub struct Iter {
    /// Directories yet to be read.
    dir_deque: VecDeque<Entry>,

    /// Files yet to be returned.
    file_deque: VecDeque<Entry>,
}


// The source iterator yields one path at a time as it walks through the source directories.
//
// It has to read each directory entirely so that it can sort the entries.  The entries are
// divided into files (or other non-directories) that can be returned directly, and
// subdirectories that can be visited when there are no more files to return.
//
// It also has to manage a stack of directories which might be partially walked.
//
// The next function then is:
//
// If there are already-queued leaf nodes (files, symlinks, etc), return the next of them.
//
// Otherwise, read a directory and queue up its results. The directory to read is the first
// on the directory queue.  It is read completely, the contents separated into leaves and
// subdirectories, and both queues are sorted.  The subdirectories are inserted onto the
// subdirectory queue to be read next.  And then the entry for the directory itself is
// returned.  The new directories are pushed at the start of the directory queue.
impl Iterator for Iter {
    type Item = io::Result<Entry>;

    fn next(&mut self) -> Option<io::Result<Entry>> {
        // Some files (or non-directories) have already been read and sorted:
        // return the next of them.
        if let Some(next_file) = self.file_deque.pop_front() {
            return Some(Ok(next_file))
        }
        // Read the next directory, or stop if there are no more.
        let dir_entry = match self.dir_deque.pop_front() {
            None => { return None },
            Some(e) => e,
        };
        let readdir = match fs::read_dir(&dir_entry.path) {
            Ok(rd) => rd,
            Err(e) => { return Some(Err(e)) },
        };
        let mut children = Vec::<(OsString, bool)>::new();
        for entry in readdir {
            match entry {
                Err(e) => { return Some(Err(e)) },
                Ok(entry) => {
                    let ft = match entry.file_type() {
                        Err(e) => { return Some(Err(e)) },
                        Ok(ft) => ft,
                    };
                    children.push((entry.file_name(), ft.is_dir()));
                },
            };
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
                self.dir_deque.insert(directory_insert_point, new_entry);
                directory_insert_point += 1;
            } else {
                self.file_deque.push_back(new_entry);
            }
        }

        // TODO: Self-check that they're in order vs the last apath emitted.
        Some(Ok(dir_entry))
    }
}

/// Iterate source files descending through a source directory.
///
/// Visit the files in a directory before descending into its children, as
/// is the defined order for files stored in an archive.  Within those files and
/// child directories, visit them according to a sorted comparison by their UTF-8
/// name.
pub fn iter(source_dir: &Path) -> Iter {
    let mut dir_deque = VecDeque::<Entry>::with_capacity(1);
    dir_deque.push_back( Entry {
        apath: "/".to_string(),
        path: source_dir.to_path_buf(),
    });
    Iter {
        file_deque: VecDeque::<Entry>::new(),
        dir_deque: dir_deque,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::itertools;
    use super::super::testfixtures::TreeFixture;

    #[test]
    fn simple_directory() {
        let tf = TreeFixture::new();
        tf.create_file("bba");
        tf.create_file("aaa");
        tf.create_dir("jam");
        tf.create_file("jam/apricot");
        tf.create_dir("jelly");
        let result = itertools::result_iter_to_vec(&mut iter(tf.path())).unwrap();
        assert_eq!(result.len(), 6);
        // First one is the root
        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[0].path, &tf.root);
        assert_eq!(&result[1].apath, "/aaa");
        assert_eq!(&result[1].path, &tf.root.join("aaa"));
        assert_eq!(&result[2].apath, "/bba");
        assert_eq!(&result[2].path, &tf.root.join("bba"));
        assert_eq!(&result[3].apath, "/jam");
        assert_eq!(&result[3].path, &tf.root.join("jam"));
        assert_eq!(&result[4].apath, "/jam/apricot");
        assert_eq!(&result[4].path, &tf.root.join("jam").join("apricot"));
        assert_eq!(&result[5].apath, "/jelly");
        assert_eq!(&result[5].path, &tf.root.join("jelly"));
    }
}
