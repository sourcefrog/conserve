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

use crate::index::stitch::Stitch;
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
    AE: EntryTrait,
    BE: EntryTrait,
{
    Left(AE),
    Right(BE),
    Both(AE, BE),
}

impl<AE, BE> MatchedEntries<AE, BE>
where
    AE: EntryTrait,
    BE: EntryTrait,
{
    pub(crate) fn to_entry_change(&self) -> EntryChange {
        match self {
            MatchedEntries::Both(ae, be) => EntryChange::diff_metadata(ae, be),
            MatchedEntries::Left(ae) => EntryChange::deleted(ae),
            MatchedEntries::Right(be) => EntryChange::added(be),
        }
    }

    // pub(crate) fn right(&self) -> Option<&BE> {
    //     match self {
    //         MatchedEntries::Right(be) => Some(be),
    //         MatchedEntries::Both(_, be) => Some(be),
    //         MatchedEntries::Left(_) => None,
    //     }
    // }

    // pub(crate) fn left(&self) -> Option<&AE> {
    //     match self {
    //         MatchedEntries::Left(ae) => Some(ae),
    //         MatchedEntries::Both(ae, _) => Some(ae),
    //         MatchedEntries::Right(_) => None,
    //     }
    // }

    pub(crate) fn into_options(self) -> (Option<AE>, Option<BE>) {
        match self {
            MatchedEntries::Left(ae) => (Some(ae), None),
            MatchedEntries::Right(be) => (None, Some(be)),
            MatchedEntries::Both(ae, be) => (Some(ae), Some(be)),
        }
    }
}

/// Zip together entries from two trees, into an iterator of [MatchedEntries].
///
/// Note that at present this only says whether files are absent from either
/// side, not whether there is a content difference.
///
/// At present, this can only diff a source tree to a stored tree, but it could
/// be genericized to diff any two trees, especially when there is a stable
/// async iterator trait.
pub struct MergeTrees {
    a: Stitch,
    b: source::Iter,
    /// Peeked next entry from `a`.
    next_a: Option<IndexEntry>,
    /// Peeked next entry from [bit].
    next_b: Option<source::Entry>,
}

impl MergeTrees {
    pub(crate) fn new(a: Stitch, b: source::Iter) -> MergeTrees {
        MergeTrees {
            a,
            b,
            next_a: None,
            next_b: None,
        }
    }

    pub(crate) async fn next(&mut self) -> Option<MatchedEntries<IndexEntry, source::Entry>> {
        // Preload next-A and next-B, if they're not already loaded.
        if self.next_a.is_none() {
            self.next_a = self.a.next().await;
        }
        if self.next_b.is_none() {
            self.next_b = self.b.next();
        }
        match (&self.next_a, &self.next_b) {
            (None, None) => None,
            (Some(_a), None) => Some(MatchedEntries::Left(self.next_a.take().unwrap())),
            (None, Some(_b)) => Some(MatchedEntries::Right(self.next_b.take().unwrap())),
            (Some(a), Some(b)) => match a.apath().cmp(b.apath()) {
                Ordering::Equal => Some(MatchedEntries::Both(
                    self.next_a.take().unwrap(),
                    self.next_b.take().unwrap(),
                )),
                Ordering::Less => Some(MatchedEntries::Left(self.next_a.take().unwrap())),
                Ordering::Greater => Some(MatchedEntries::Right(self.next_b.take().unwrap())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    // use crate::monitor::test::TestMonitor;
    // use crate::test_fixtures::*;
    // use crate::*;

    // use super::*;

    // TODO: Merge (maybe using proptest) some stored and live trees.

    // #[test]
    // fn merge_entry_trees() {
    //     let ta = TreeFixture::new();
    //     let tb = TreeFixture::new();
    //     let monitor = TestMonitor::arc();
    //     let di = MergeTrees::new(
    //         ta.live_tree()
    //             .iter_entries(Apath::root(), Exclude::nothing(), monitor.clone())
    //             .unwrap(),
    //         tb.live_tree()
    //             .iter_entries(Apath::root(), Exclude::nothing(), monitor.clone())
    //             .unwrap(),
    //     )
    //     .collect::<Vec<_>>();
    //     assert_eq!(di.len(), 1);
    //     match &di[0] {
    //         MatchedEntries::Both(ae, be) => {
    //             assert_eq!(ae.kind(), Kind::Dir);
    //             assert_eq!(be.kind(), Kind::Dir);
    //             assert_eq!(ae.apath(), "/");
    //             assert_eq!(be.apath(), "/");
    //         }
    //         other => panic!("unexpected {other:#?}"),
    //     }
    // }

    // TODO: More tests of various diff situations.
}
