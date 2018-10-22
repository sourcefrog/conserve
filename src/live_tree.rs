// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Find source files within a source directory, in apath order.

use std::collections::vec_deque::VecDeque;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

use globset::GlobSet;

/// A real tree on the filesystem, for use as a backup source or restore destination.
#[derive(Clone)]
pub struct LiveTree {
    path: PathBuf,
    report: Report,
    excludes: GlobSet,
}

impl LiveTree {
    pub fn open<P: AsRef<Path>>(path: P, report: &Report) -> Result<LiveTree> {
        // TODO: Maybe fail here if the root doesn't exist or isn't a directory?
        Ok(LiveTree {
            path: path.as_ref().to_path_buf(),
            report: report.clone(),
            excludes: excludes::excludes_nothing(),
        })
    }

    /// Return a new LiveTree which when listed will ignore certain files.
    ///
    /// This replaces any previous exclusions.
    pub fn with_excludes(self, excludes: GlobSet) -> LiveTree {
        LiveTree { excludes, ..self }
    }
}

impl tree::ReadTree for LiveTree {
    type E = Entry;
    type I = Iter;
    type R = std::fs::File;

    /// Iterate source files descending through a source directory.
    ///
    /// Visit the files in a directory before descending into its children, as
    /// is the defined order for files stored in an archive.  Within those files and
    /// child directories, visit them according to a sorted comparison by their UTF-8
    /// name.
    ///
    /// The `Iter` has its own `Report` of how many directories and files were visited.
    fn iter_entries(&self, report: &Report) -> Result<Self::I> {
        let root_metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(e) => {
                self.report.problem(&format!("{}", e));
                return Err(e.into());
            }
        };
        let root_entry = Entry {
            apath: Apath::from("/"),
            path: self.path.clone(),
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
            entry_deque,
            dir_deque,
            report: report.clone(),
            check_order: apath::CheckOrder::new(),
            excludes: self.excludes.clone(),
        })
    }

    fn file_contents(&self, entry: &Self::E) -> Result<Self::R> {
        use entry::Entry;
        assert_eq!(entry.kind(), Kind::File);
        let mut path = self.path.clone();
        path.push(&entry.apath[1..]);
        Ok(fs::File::open(&path)?)
    }

    fn estimate_count(&self) -> Result<u64> {
        // TODO: This stats the file and builds an entry about them, just to
        // throw it away. We could perhaps change the iter to optionally do
        // less work.

        // Make a new report so it doesn't pollute the report for the actual
        // backup work.
        Ok(self.iter_entries(&Report::new())?.count() as u64)
    }
}

impl HasReport for LiveTree {
    fn report(&self) -> &Report {
        &self.report
    }
}

impl fmt::Debug for LiveTree {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("LiveTree")
            .field("path", &self.path)
            .finish()
    }
}

/// An entry in a live tree, describing a real file etc on disk.
#[derive(Clone)]
pub struct Entry {
    /// Conserve apath, relative to the top-level directory.
    pub apath: Apath,

    /// Possibly absolute path through which the file can be opened.
    pub path: PathBuf,

    /// stat-like structure including kind, mtime, etc.
    pub metadata: fs::Metadata,
}

impl entry::Entry for Entry {
    fn apath(&self) -> Apath {
        // TODO: Better to just return a reference with the same lifetime, once index entries can
        // support that.
        self.apath.clone()
    }

    fn kind(&self) -> Kind {
        if self.metadata.is_file() {
            Kind::File
        } else if self.metadata.is_dir() {
            Kind::Dir
        } else if self.metadata.file_type().is_symlink() {
            Kind::Symlink
        } else {
            Kind::Unknown
        }
    }

    fn unix_mtime(&self) -> Option<u64> {
        self.metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|dur| Some(dur.as_secs()))
    }

    fn symlink_target(&self) -> Option<String> {
        // TODO: Record a problem and log a message if the target is not decodable, rather than
        // panicing.
        // TODO: Also return a Result if the link can't be read?
        match self.kind() {
            Kind::Symlink => Some(
                fs::read_link(&self.path)
                    .unwrap()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            ),
            _ => None,
        }
    }
}

impl fmt::Debug for Entry {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("live_tree::Entry")
            .field("apath", &self.apath)
            .field("path", &self.path)
            .finish()
    }
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

    /// Check that emitted paths are in the right order.
    check_order: apath::CheckOrder,

    /// glob pattern to skip in iterator
    excludes: GlobSet,
}

impl Iter {
    fn visit_next_directory(&mut self, dir_entry: &Entry) -> Result<()> {
        self.report.increment("source.visited.directories", 1);
        let mut children = Vec::<(OsString, bool, Apath)>::new();
        for entry in fs::read_dir(&dir_entry.path)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let mut path = String::from(dir_entry.apath.clone());
            if path != "/" {
                path.push('/');
            }
            // TODO: Don't be lossy, error if not convertible.
            path.push_str(&entry.file_name().to_string_lossy());

            if self.excludes.is_match(&path) {
                if ft.is_file() {
                    self.report.increment("skipped.excluded.files", 1);
                } else if ft.is_dir() {
                    self.report.increment("skipped.excluded.directories", 1);
                } else if ft.is_symlink() {
                    self.report.increment("skipped.excluded.symlinks", 1);
                }
                continue;
            }
            children.push((entry.file_name(), ft.is_dir(), Apath::from(path)));
        }

        children.sort();
        let mut directory_insert_point = 0;
        for (child_name, is_dir, apath) in children {
            let child_path = dir_entry.path.join(&child_name).to_path_buf();
            let metadata = match fs::symlink_metadata(&child_path) {
                Ok(metadata) => metadata,
                Err(e) => {
                    self.report
                        .problem(&format!("source metadata error on {:?}: {}", child_path, e));
                    self.report.increment("source.error.metadata", 1);
                    continue;
                }
            };
            let new_entry = Entry {
                apath,
                path: child_path,
                metadata,
            };
            if is_dir {
                self.dir_deque
                    .insert(directory_insert_point, new_entry.clone());
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
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Result<Entry>> {
        loop {
            if let Some(entry) = self.entry_deque.pop_front() {
                // Have already found some entries, so just return the first.
                self.report.increment("source.selected", 1);
                // Sanity check that all the returned paths are in correct order.
                self.check_order.check(&entry.apath);
                return Some(Ok(entry));
            }

            // No entries already queued, visit a new directory to try to refill the queue.
            if let Some(entry) = self.dir_deque.pop_front() {
                if let Err(e) = self.visit_next_directory(&entry) {
                    return Some(Err(e));
                }
            } else {
                // No entries queued and no more directories to visit.
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use test_fixtures::TreeFixture;

    #[test]
    fn open_tree() {
        let tf = TreeFixture::new();
        let lt = LiveTree::open(tf.path(), &Report::new()).unwrap();
        assert_eq!(
            format!("{:?}", &lt),
            format!("LiveTree {{ path: {:?} }}", tf.path())
        );
    }

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
        let lt = LiveTree::open(tf.path(), &report).unwrap();
        let mut source_iter = lt.iter_entries(&report).unwrap();
        let result = source_iter.by_ref().collect::<Result<Vec<_>>>().unwrap();
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

        assert_eq!(
            format!("{:?}", &result[6]),
            format!(
                "live_tree::Entry {{ apath: Apath({:?}), path: {:?} }}",
                "/jam/apricot",
                &tf.root.join("jam").join("apricot")
            )
        );

        assert_eq!(report.get_count("source.visited.directories"), 4);
        assert_eq!(report.get_count("source.selected"), 7);
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

        let excludes = excludes::from_strings(&["/**/fooo*", "/**/ba[pqr]", "/**/*bas"]).unwrap();

        let lt = LiveTree::open(tf.path(), &report)
            .unwrap()
            .with_excludes(excludes);
        let mut source_iter = lt.iter_entries(&report).unwrap();
        let result = source_iter.by_ref().collect::<Result<Vec<_>>>().unwrap();

        // First one is the root
        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[0].path, &tf.root);
        assert_eq!(&result[1].apath, "/baz");
        assert_eq!(&result[1].path, &tf.root.join("baz"));
        assert_eq!(&result[2].apath, "/baz/test");
        assert_eq!(&result[2].path, &tf.root.join("baz").join("test"));
        assert_eq!(result.len(), 3);

        assert_eq!(
            format!("{:?}", &result[2]),
            format!(
                "live_tree::Entry {{ apath: Apath({:?}), path: {:?} }}",
                "/baz/test",
                &tf.root.join("baz").join("test")
            )
        );

        assert_eq!(
            2,
            report
                .borrow_counts()
                .get_count("source.visited.directories",)
        );
        assert_eq!(3, report.borrow_counts().get_count("source.selected"));
        assert_eq!(
            4,
            report.borrow_counts().get_count("skipped.excluded.files")
        );
        assert_eq!(
            1,
            report
                .borrow_counts()
                .get_count("skipped.excluded.directories",)
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks() {
        let tf = TreeFixture::new();
        tf.create_symlink("from", "to");
        let report = Report::new();

        let lt = LiveTree::open(tf.path(), &report).unwrap();
        let result = lt
            .iter_entries(&report)
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[0].path, &tf.root);

        assert_eq!(&result[1].apath, "/from");
        assert_eq!(&result[1].path, &tf.root.join("from"));
    }
}
