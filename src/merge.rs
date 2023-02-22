// Copyright 2018-2023 Martin Pool.

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

/// When merging entries from two trees a particular apath might
/// be present in either or both trees.
///
/// Unlike the [Change] struct, this contains the full entry rather than
/// just metadata, and in particular will contain the block addresses for
/// [IndexEntry].
#[derive(Debug, PartialEq, Eq)]
pub enum MatchedEntries<AE, BE>
where
    AE: Entry,
    BE: Entry,
{
    Left(AE),
    Right(BE),
    Both(AE, BE),
}

impl<AE, BE> MatchedEntries<AE, BE>
where
    AE: Entry,
    BE: Entry,
{
    pub(crate) fn to_entry_change(&self) -> EntryChange {
        match self {
            MatchedEntries::Both(ae, be) => EntryChange::diff_metadata(ae, be),
            MatchedEntries::Left(ae) => EntryChange::deleted(ae),
            MatchedEntries::Right(be) => EntryChange::added(be),
        }
    }
}

/// Zip together entries from two trees, into an iterator of [MatchedEntries].
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
    /// Peeked next entry from [ait].
    na: Option<AE>,
    /// Peeked next entry from [bit].
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
    type Item = MatchedEntries<AE, BE>;

    fn next(&mut self) -> Option<Self::Item> {
        // Preload next-A and next-B, if they're not already loaded.
        if self.na.is_none() {
            self.na = self.ait.next();
        }
        if self.nb.is_none() {
            self.nb = self.bit.next();
        }
        match (&self.na, &self.nb) {
            (None, None) => None,
            (Some(_a), None) => Some(MatchedEntries::Left(self.na.take().unwrap())),
            (None, Some(_b)) => Some(MatchedEntries::Right(self.nb.take().unwrap())),
            (Some(a), Some(b)) => match a.apath().cmp(b.apath()) {
                Ordering::Equal => Some(MatchedEntries::Both(
                    self.na.take().unwrap(),
                    self.nb.take().unwrap(),
                )),
                Ordering::Less => Some(MatchedEntries::Left(self.na.take().unwrap())),
                Ordering::Greater => Some(MatchedEntries::Right(self.nb.take().unwrap())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::*;
    use crate::*;

    use super::MatchedEntries;

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
        match &di[0] {
            MatchedEntries::Both(ae, be) => {
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
