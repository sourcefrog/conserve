// Copyright 2018, 2019 Martin Pool.

//! Merge two trees by iterating them in lock step.

use std::cmp::Ordering;

use crate::*;

#[derive(Debug, PartialEq, Eq)]
pub enum MergedEntryKind {
    LeftOnly,
    RightOnly,
    Both,
    // TODO: Perhaps also include the tree-specific entry kind?
}

use self::MergedEntryKind::*;

#[derive(Debug, PartialEq, Eq)]
pub struct MergedEntry {
    // TODO: Add accessors rather than making these public?
    // TODO: Include the original entries from either side?
    pub apath: Apath,
    pub kind: MergedEntryKind,
}

/// Zip together entries from two trees, into an iterator of MergedEntryKind.
///
/// Note that at present this only says whether files are absent from either
/// side, not whether there is a content difference.
pub fn iter_merged_entries<AT, BT>(a: &AT, b: &BT, report: &Report) -> Result<MergeTrees<AT, BT>>
where
    AT: ReadTree,
    BT: ReadTree,
{
    Ok(MergeTrees {
        ait: a.iter_entries(report)?,
        bit: b.iter_entries(report)?,
        na: None,
        nb: None,
    })
}

pub struct MergeTrees<AT: ReadTree, BT: ReadTree> {
    ait: AT::I,
    bit: BT::I,

    // Read in advance entries from A and B.
    na: Option<AT::E>,
    nb: Option<BT::E>,
}

impl<AT, BT> Iterator for MergeTrees<AT, BT>
where
    AT: ReadTree,
    BT: ReadTree,
{
    type Item = Result<MergedEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: Count into report?
        let ait = &mut self.ait;
        let bit = &mut self.bit;
        // Preload next-A and next-B, if they're not already
        // loaded.
        if self.na.is_none() {
            self.na = match ait.next() {
                None => None,
                Some(Err(e)) => return Some(Err(e)),
                Some(Ok(i)) => Some(i),
            }
        }
        if self.nb.is_none() {
            self.nb = match bit.next() {
                None => None,
                Some(Err(e)) => return Some(Err(e)),
                Some(Ok(i)) => Some(i),
            }
        }
        if self.na.is_none() {
            if self.nb.is_none() {
                None
            } else {
                let tb = self.nb.take().unwrap();
                Some(Ok(MergedEntry {
                    apath: tb.apath(),
                    kind: RightOnly,
                }))
            }
        } else if self.nb.is_none() {
            Some(Ok(MergedEntry {
                apath: self.na.take().unwrap().apath(),
                kind: LeftOnly,
            }))
        } else {
            let pa = self.na.as_ref().unwrap().apath();
            let pb = self.nb.as_ref().unwrap().apath();
            match pa.cmp(&pb) {
                Ordering::Equal => {
                    self.na.take();
                    self.nb.take();
                    Some(Ok(MergedEntry {
                        apath: pa,
                        kind: Both,
                    }))
                }
                Ordering::Less => {
                    self.na.take().unwrap();
                    Some(Ok(MergedEntry {
                        apath: pa,
                        kind: LeftOnly,
                    }))
                }
                Ordering::Greater => {
                    self.nb.take().unwrap();
                    Some(Ok(MergedEntry {
                        apath: pb,
                        kind: RightOnly,
                    }))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MergedEntry;
    use super::MergedEntryKind::*;
    use crate::test_fixtures::*;
    use crate::*;

    #[test]
    fn diff_empty_trees() {
        let ta = TreeFixture::new();
        let tb = TreeFixture::new();
        let report = Report::new();

        let di = iter_merged_entries(&ta.live_tree(), &tb.live_tree(), &report)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(di.len(), 1);
        assert_eq!(
            *di[0].as_ref().unwrap(),
            MergedEntry {
                apath: "/".into(),
                kind: Both,
            }
        );
    }

    // TODO: More tests of various diff situations.
}
