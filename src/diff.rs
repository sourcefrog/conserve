// Copyright 2018, 2019 Martin Pool.

// TODO: Perhaps split out zipped iteration of entries (without looking
// at their metadata or contents), from actual diff-ing.

use std::cmp::Ordering;

use crate::*;

///! Diff two trees by walking them in order, in parallel.

#[derive(Debug, PartialEq, Eq)]
pub enum DiffEntryKind {
    LeftOnly,
    RightOnly,
    Both,

    // TODO: Perhaps also include the tree-specific entry kind?
}

use self::DiffEntryKind::*;

#[derive(Debug, PartialEq, Eq)]
pub struct DiffEntry {
    // TODO: Add accessors rather than making these public?
    pub apath: Apath,
    pub kind: DiffEntryKind,
}

/// Zip together entries from two trees, into an iterator of
/// either Results or DiffEntry.
pub fn diff<AT, BT>(a: &AT, b: &BT, report: &Report) -> Result<DiffTrees<AT, BT>>
where
    AT: ReadTree,
    BT: ReadTree,
{
    Ok(DiffTrees {
        ait: a.iter_entries(report)?,
        bit: b.iter_entries(report)?,
        na: None,
        nb: None,
    })
}

pub struct DiffTrees<AT: ReadTree, BT: ReadTree> {
    ait: AT::I,
    bit: BT::I,

    // Read in advance entries from A and B.
    na: Option<AT::E>,
    nb: Option<BT::E>,
}

impl<AT, BT> Iterator for DiffTrees<AT, BT>
where
    AT: ReadTree,
    BT: ReadTree,
{
    type Item = Result<DiffEntry>;

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
                Some(Ok(DiffEntry {
                    apath: tb.apath(),
                    kind: RightOnly,
                }))
            }
        } else if self.nb.is_none() {
            Some(Ok(DiffEntry {
                apath: self.na.take().unwrap().apath(),
                kind: LeftOnly,
            }))
        } else {
            let pa = self.na.as_ref().unwrap().apath();
            let pb = self.nb.as_ref().unwrap().apath();
            match pa.cmp(&pb) {
                Ordering::Equal => {
                    (self.na.take(), self.nb.take());
                    Some(Ok(DiffEntry {
                        apath: pa.clone(),
                        kind: Both,
                    }))
                }
                Ordering::Less => {
                    self.na.take().unwrap();
                    Some(Ok(DiffEntry {
                        apath: pa,
                        kind: LeftOnly,
                    }))
                }
                Ordering::Greater => {
                    self.nb.take().unwrap();
                    Some(Ok(DiffEntry {
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
    use super::DiffEntry;
    use super::DiffEntryKind::*;
    use crate::test_fixtures::*;
    use crate::*;

    #[test]
    fn diff_empty_trees() {
        let ta = TreeFixture::new();
        let tb = TreeFixture::new();
        let report = Report::new();

        let di = diff(&ta.live_tree(), &tb.live_tree(), &report)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(di.len(), 1);
        assert_eq!(
            *di[0].as_ref().unwrap(),
            DiffEntry {
                apath: "/".into(),
                kind: Both,
            }
        );
    }

    // TODO: More tests of various diff situations.
}
