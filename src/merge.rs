// Copyright 2018, 2019, 2020, 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Merge two trees by iterating them in lock step.
//!
//! This is a foundation for showing the diff between a stored and
//! live tree, or storing an incremental backup.

use std::cmp::Ordering;

use crate::*;

// TODO: Fold in to EntryChange.
#[derive(Debug, PartialEq, Eq)]
pub enum MergedEntryKind<AE, BE>
where
    AE: Entry,
    BE: Entry,
{
    LeftOnly(AE),
    RightOnly(BE),
    Both(AE, BE),
}

use self::MergedEntryKind::*;

// TODO: Fold in to EntryChange.
#[derive(Debug, PartialEq, Eq)]
pub struct MergedEntry<AE, BE>
where
    AE: Entry,
    BE: Entry,
{
    pub apath: Apath,
    pub kind: MergedEntryKind<AE, BE>,
}

/// Zip together entries from two trees, into an iterator of MergedEntryKind.
///
/// Note that at present this only says whether files are absent from either
/// side, not whether there is a content difference.
pub struct MergeTrees<AE, BE, AIT, BIT>
where
    AE: Entry,
    BE: Entry,
    AIT: Iterator<Item = AE>,
    BIT: Iterator<Item = BE>,
{
    ait: AIT,
    bit: BIT,
    // Read in advance entries from A and B.
    na: Option<AE>,
    nb: Option<BE>,
}

impl<AE, BE, AIT, BIT> MergeTrees<AE, BE, AIT, BIT>
where
    AE: Entry,
    BE: Entry,
    AIT: Iterator<Item = AE>,
    BIT: Iterator<Item = BE>,
{
    pub fn new(ait: AIT, bit: BIT) -> MergeTrees<AE, BE, AIT, BIT> {
        MergeTrees {
            ait,
            bit,
            na: None,
            nb: None,
        }
    }
}

impl<AE, BE, AIT, BIT> Iterator for MergeTrees<AE, BE, AIT, BIT>
where
    AE: Entry,
    BE: Entry,
    AIT: Iterator<Item = AE>,
    BIT: Iterator<Item = BE>,
{
    type Item = MergedEntry<AE, BE>;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: Stats about the merge.
        let ait = &mut self.ait;
        let bit = &mut self.bit;
        // Preload next-A and next-B, if they're not already
        // loaded.
        //
        // TODO: Perhaps use `Peekable` instead of keeping a readahead here?
        if self.na.is_none() {
            self.na = ait.next();
        }
        if self.nb.is_none() {
            self.nb = bit.next();
        }
        if self.na.is_none() {
            if self.nb.is_none() {
                None
            } else {
                let tb = self.nb.take().unwrap();
                Some(MergedEntry {
                    apath: tb.apath().clone(),
                    kind: RightOnly(tb),
                })
            }
        } else if self.nb.is_none() {
            let ta = self.na.take().unwrap();
            Some(MergedEntry {
                apath: ta.apath().clone(),
                kind: LeftOnly(ta),
            })
        } else {
            let pa = self.na.as_ref().unwrap().apath().clone();
            let pb = self.nb.as_ref().unwrap().apath().clone();
            match pa.cmp(&pb) {
                Ordering::Equal => Some(MergedEntry {
                    apath: pa,
                    kind: Both(self.na.take().unwrap(), self.nb.take().unwrap()),
                }),
                Ordering::Less => Some(MergedEntry {
                    apath: pa,
                    kind: LeftOnly(self.na.take().unwrap()),
                }),
                Ordering::Greater => Some(MergedEntry {
                    apath: pb,
                    kind: RightOnly(self.nb.take().unwrap()),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MergedEntryKind::*;
    use crate::test_fixtures::*;
    use crate::*;

    #[test]
    fn merge_entry_trees() {
        let ta = TreeFixture::new();
        let tb = TreeFixture::new();
        let di = MergeTrees::new(
            ta.live_tree()
                .iter_entries(Apath::root(), Exclude::nothing())
                .unwrap(),
            tb.live_tree()
                .iter_entries(Apath::root(), Exclude::nothing())
                .unwrap(),
        )
        .collect::<Vec<_>>();
        assert_eq!(di.len(), 1);
        assert_eq!(di[0].apath, "/");
        match &di[0].kind {
            Both(ae, be) => {
                assert_eq!(ae.kind(), Kind::Dir);
                assert_eq!(be.kind(), Kind::Dir);
                assert_eq!(ae.apath(), "/");
                assert_eq!(be.apath(), "/");
            }
            other => panic!("unexpected {other:#?}"),
        }
    }

    // TODO: More tests of various diff situations.
}
