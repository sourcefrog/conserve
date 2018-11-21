// Copyright 2018 Martin Pool.

use std::cmp::Ordering;

use crate::*;

///! Diff two trees.

/// Zip together entries from two trees, returning
/// pairs of options.
///
/// This should, later, be an iterator of DiffEntry.
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
    type Item = Result<(Option<AT::E>, Option<BT::E>)>;

    fn next(&mut self) -> Option<Self::Item> {
        let ait = &mut self.ait;
        let bit = &mut self.bit;
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
                Some(Ok((None, self.nb.take())))
            }
        } else if self.nb.is_none() {
            Some(Ok((self.na.take(), None)))
        } else {
            let pa = self.na.as_ref().unwrap().apath();
            let pb = self.nb.as_ref().unwrap().apath();
            match pa.cmp(&pb) {
                Ordering::Equal => Some(Ok((self.na.take(), self.nb.take()))),
                Ordering::Less => Some(Ok((self.na.take(), None))),
                Ordering::Greater => Some(Ok((None, self.nb.take()))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(&di[0].unwrap().0.unwrap().apath(), "/");
    }
}
