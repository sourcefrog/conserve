use serde::{self, Serialize};
use jiff::Timestamp;

use crate::{Apath, EntryTrait, Kind, Owner, UnixMode, entry::KindMeta};

/// A description of a file, directory, or symlink in a source tree.
#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
pub struct Entry {
    pub(crate) apath: Apath,

    /// Is it a file, dir, or symlink, and for files the size and for symlinks the target.
    #[serde(flatten)]
    pub(crate) kind_meta: KindMeta,

    /// Modification time.
    pub(crate) mtime: Timestamp,
    pub(crate) unix_mode: UnixMode,
    #[serde(flatten)]
    pub(crate) owner: Owner,
}

impl EntryTrait for Entry {
    fn apath(&self) -> &Apath {
        &self.apath
    }

    fn kind(&self) -> Kind {
        Kind::from(&self.kind_meta)
    }

    fn mtime(&self) -> Timestamp {
        self.mtime
    }

    fn size(&self) -> Option<u64> {
        if let KindMeta::File { size } = self.kind_meta {
            Some(size)
        } else {
            None
        }
    }

    fn symlink_target(&self) -> Option<&str> {
        match &self.kind_meta {
            KindMeta::Symlink { target } => Some(target),
            _ => None,
        }
    }

    fn unix_mode(&self) -> UnixMode {
        self.unix_mode
    }

    fn owner(&self) -> &Owner {
        &self.owner
    }

    fn listing_json(&self) -> serde_json::Value {
        // TODO: Emit the time as mtime and mtime_nanos.
        serde_json::value::to_value(self).unwrap()
    }
}
