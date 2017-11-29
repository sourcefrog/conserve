// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Find source files within a source directory, in apath order.

use std::collections::vec_deque::VecDeque;
use std::fmt;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time;

use super::*;

use globset::GlobSet;

/// An entry found in the source directory.
#[derive(Clone)]
pub struct Entry {
    /// Conserve apath, relative to the top-level directory.
    pub apath: Apath,

    /// Possibly absolute path through which the file can be opened.
    pub path: PathBuf,

    /// stat-like structure including kind, mtime, etc.
    pub metadata: fs::Metadata,
}


impl Entry {
    /// Return Unix-format mtime if possible.
    pub fn unix_mtime(&self) -> Option<u64> {
        self.metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(time::UNIX_EPOCH).ok())
            .and_then(|dur| Some(dur.as_secs()))
    }
}


impl fmt::Debug for Entry {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("sources::Entry")
            .field("apath", &self.apath)
            .field("path", &self.path)
            .finish()
    }
}

/// Recursive iterator of the contents of a source directory.
pub struct Iter {
    /// Directories yet to be visited.
    dir_deque: VecDeque<Entry>,

    /// Direct children of the current directory yet to be returned.
    entry_deque: VecDeque<Entry>,

    /// Count of directories and files visited by this iterator.
    report: Report,

    /// Copy of the last-emitted apath, for the purposes of checking they're in apath order.
    last_apath: Option<Apath>,

    /// glob pattern to skip in iterator
    excludes: GlobSet
}

// TODO: Implement Debug on Iter.


impl Iter {
    fn unchecked_next(&mut self) -> Option<io::Result<Entry>> {
        loop {
            if let Some(entry) = self.entry_deque.pop_front() {
                // Have already found some entries and just need to return them.
                self.report.increment("source.selected", 1);
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
        self.report.increment("source.visited.directories", 1);
        let mut children = Vec::<(OsString, bool, Apath)>::new();
        for entry in readdir {
            let entry = try!(entry);
            let ft = try!(entry.file_type());
            let mut path = dir_entry.apath.to_string().clone();
            if path != "/" {
                path.push('/');
            }
            // TODO: Don't be lossy, error if not convertible.
            path.push_str(&entry.file_name().to_string_lossy());

            if self.excludes.is_match(&path) {
                if ft.is_dir() {
                    self.report.increment("skipped.excluded.directories", 1);
                } else {
                    self.report.increment("skipped.excluded.files", 1);
                }
                continue;
            }
            children.push((entry.file_name(), ft.is_dir(), Apath::from_string(&path)));
        }
        children.sort();
        let mut directory_insert_point = 0;
        for (child_name, is_dir, apath) in children {
            let child_path = dir_entry.path.join(&child_name).to_path_buf();
            let metadata = match fs::symlink_metadata(&child_path) {
                Ok(metadata) => metadata,
                Err(e) => {
                    warn!("{}", e);
                    self.report.increment("source.error.metadata", 1);
                    continue;
                }
            };
            let new_entry = Entry {
                apath: apath,
                path: child_path,
                metadata: metadata,
            };
            if is_dir {
                self.dir_deque.insert(directory_insert_point, new_entry.clone());
                directory_insert_point += 1;
            }
            self.entry_deque.push_back(new_entry);
        }
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
                    assert!(last_apath < &entry.apath,
                            "sources returned out of order: {:?} >= {:?}",
                            last_apath,
                            entry.apath);
                }
                self.last_apath = Some(entry.apath.clone());
                Some(Ok(entry))
            }
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
pub fn iter(source_dir: &Path, report: &Report, excludes: &GlobSet) -> io::Result<Iter> {
    let root_metadata = match fs::symlink_metadata(&source_dir) {
        Ok(metadata) => metadata,
        Err(e) => {
            warn!("{}", e);
            return Err(e);
        }
    };
    let root_entry = Entry {
        apath: Apath::from_string("/"),
        path: source_dir.to_path_buf(),
        metadata: root_metadata,
    };
    // Preload iter to return the root and then recurse into it.
    let mut entry_deque: VecDeque<Entry> = VecDeque::<Entry>::new();
    entry_deque.push_back(root_entry.clone());
    // TODO: Consider the case where the root is not actually a directory?
    // Should that be supported?
    let mut dir_deque = VecDeque::<Entry>::new();
    dir_deque.push_back(root_entry);
    Ok(Iter {
        entry_deque: entry_deque,
        dir_deque: dir_deque,
        report: report.clone(),
        last_apath: None,
        excludes: excludes.clone()
    })
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::iter;
    use super::super::*;
    use testfixtures::TreeFixture;

    #[test]
    fn simple_directory() {
        let tf = TreeFixture::new();
        tf.create_file("bba");
        tf.create_file("aaa");
        tf.create_dir("jam");
        tf.create_file("jam/apricot");
        tf.create_dir("jelly");
        tf.create_dir("jam/.etc");
        let report = Report::new();
        let mut source_iter = iter(tf.path(), &report, &excludes::produce_no_excludes()).unwrap();
        let result = source_iter.by_ref().collect::<io::Result<Vec<_>>>().unwrap();
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

        assert_eq!(format!("{:?}", &result[6]),
                   format!("sources::Entry {{ apath: Apath({:?}), path: {:?} }}",
                           "/jam/apricot",
                           &tf.root.join("jam").join("apricot")));

        assert_eq!(report.borrow_counts().get_count("source.visited.directories"),
                   4);
        assert_eq!(report.borrow_counts().get_count("source.selected"), 7);
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
        let report = Report::new();

        let vec = vec!["/**/fooo*", "/**/ba[pqr]", "/**/*bas"];
        let excludes = excludes::produce_excludes(vec).unwrap();

        let mut source_iter = iter(tf.path(), &report, &excludes).unwrap();
        let result = source_iter.by_ref().collect::<io::Result<Vec<_>>>().unwrap();

        // First one is the root
        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[0].path, &tf.root);
        assert_eq!(&result[1].apath, "/baz");
        assert_eq!(&result[1].path, &tf.root.join("baz"));
        assert_eq!(&result[2].apath, "/baz/test");
        assert_eq!(&result[2].path, &tf.root.join("baz").join("test"));
        assert_eq!(result.len(), 3);

        assert_eq!(format!("{:?}", &result[2]),
                   format!("sources::Entry {{ apath: Apath({:?}), path: {:?} }}",
                           "/baz/test",
                           &tf.root.join("baz").join("test")));

        assert_eq!(2, report.borrow_counts().get_count("source.visited.directories"));
        assert_eq!(3, report.borrow_counts().get_count("source.selected"));
        assert_eq!(4, report.borrow_counts().get_count("skipped.excluded.files"));
        assert_eq!(1, report.borrow_counts().get_count("skipped.excluded.directories"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks() {
        let tf = TreeFixture::new();
        tf.create_symlink("from", "to");
        let report = Report::new();

        let result = iter(tf.path(), &report, &excludes::produce_no_excludes())
            .unwrap()
            .collect::<io::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[0].path, &tf.root);

        assert_eq!(&result[1].apath, "/from");
        assert_eq!(&result[1].path, &tf.root.join("from"));
    }
}
