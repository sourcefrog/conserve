// TODO: This uses single indexes, but for consistency with the rest of Conserve
// it should probably use stitched indexes, so that it sees the continuation
// of interrupted backups.

// TODO: Unit tests.

// This is currently only used by projfs, but is not inherently Windows-specific.
#![cfg_attr(not(windows), allow(unused))]

use std::cmp::Ordering;

use crate::{Apath, IndexRead, Result};

#[derive(Debug)]
struct HunkIndexMeta {
    index: u32,

    start_path: Apath,
    end_path: Apath,
}

/// An index over all available hunks available in an index
/// for speeding up sub-dir iterations and locating
/// path metadata.
pub struct IndexHunkIndex {
    hunks: Vec<HunkIndexMeta>,
}

impl IndexHunkIndex {
    /// Index all available hunks from the read index.
    ///
    /// Note:
    /// Depending on the index size this might not be a cheap operation
    /// as we loop through every hunk and read its contents.
    pub fn from_index(index: &IndexRead) -> Result<Self> {
        let mut hunk_info = index
            .hunks_available()?
            .into_iter()
            .map(move |hunk_index| {
                let mut index = index.duplicate();
                let entries = index.read_hunk(hunk_index)?;
                let meta_info = if let Some(entries) = entries {
                    if let (Some(first), Some(last)) = (entries.first(), entries.last()) {
                        Some(HunkIndexMeta {
                            index: hunk_index,

                            start_path: first.apath.clone(),
                            end_path: last.apath.clone(),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(meta_info)
            })
            .filter_map(Result::ok)
            .flatten()
            .collect::<Vec<_>>();

        /* After parallel execution bring all hunks back into order */
        hunk_info.sort_by_key(|info| info.index);
        Ok(Self { hunks: hunk_info })
    }

    fn find_hunk_index_for_file(&self, path: &Apath) -> Option<usize> {
        let hunk_index = self.hunks.binary_search_by(|entry| {
            match (entry.start_path.cmp(path), entry.end_path.cmp(path)) {
                (Ordering::Less, Ordering::Less) => Ordering::Less,
                (Ordering::Greater, Ordering::Greater) => Ordering::Greater,
                _ => Ordering::Equal,
            }
        });

        /*
         * If we do not have an exact match, no hunk contains the path we
         * are looking for.
         */
        hunk_index.ok()
    }

    /// Locate the hunk index of the hunk which should contain the file metadata
    /// for a given path.
    ///
    /// Note:
    /// To validate file existence it's required to parse the hunk contents and actually
    /// check of the file path exist. This function only returns the hunk index where the file
    /// should be located, if it exists.
    pub fn find_hunk_for_file(&self, path: &Apath) -> Option<u32> {
        self.find_hunk_index_for_file(path)
            .map(|index| self.hunks[index].index)
    }

    /// Locate the hunks where the file metadata for the directory contents of a particular directory
    /// are stored.
    ///
    /// Note:
    /// To validate directory existence it's required to parse the hunk contents and actually
    /// check of the directory path exist.
    pub fn find_hunks_for_subdir(&self, path: &Apath, recursive: bool) -> Vec<u32> {
        /*
         * Appending an empty string to the path allows us to search for the first file
         * in the target directory. This is needed as a file and a directory with the same name are not
         * stored in succession.
         *
         * Example (from the apath test):
         *  - /b/a
         *  - /b/b
         *  - /b/c
         *  - /b/a/c
         *  - /b/b/c
         */
        let search_path = path.append("");
        let directory_start_hunk = match self.find_hunk_index_for_file(&search_path) {
            Some(index) => index,
            None => return vec![],
        };

        let mut result = Vec::new();
        result.push(self.hunks[directory_start_hunk].index);
        for hunk in &self.hunks[directory_start_hunk + 1..] {
            if !path.is_prefix_of(&hunk.start_path) {
                break;
            }

            if !recursive && hunk.start_path[path.len() + 1..].contains('/') {
                /* hunk does already contain directory content */
                break;
            }

            /* hunk still contains subtree elements of that path */
            result.push(hunk.index);
        }

        result
    }
}
