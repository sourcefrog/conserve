//! Experimental support for storing indexes as flatbuffers.

#[allow(dead_code, unused_imports, clippy::all)]
pub(crate) mod index_generated;

use std::collections::HashMap;
use std::{fs::File, io::Write};

use tracing::{debug, trace};

use crate::*;

use index_generated::conserve::index as gen;

pub fn write_index(st: &StoredTree, mut out_file: File) -> Result<()> {
    let all_entries: Vec<_> = st
        .iter_entries(Apath::root(), Exclude::nothing())?
        .collect();
    debug!("Loaded {} entries", all_entries.len());

    // Map from hash to serialized location, so that hashes are stored only once.
    let mut hash_to_fb: HashMap<BlockHash, _> = HashMap::new();
    let mut name_to_pb: HashMap<String, _> = HashMap::new();

    // TODO: Possibly, we should have the serialized layout have all the apaths together,
    // all the hashes, all the user/group names, and then all the structs. That seems
    // possible and would probably help bytewise compression..

    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(200 * all_entries.len());
    trace!("Allocated builder");
    let fb_entries: Vec<_> = all_entries
        .into_iter()
        .map(|entry| {
            let addrs = entry
                .addrs
                .iter()
                .map(|addr| {
                    let hash = *hash_to_fb
                        .entry(addr.hash.clone())
                        .or_insert_with(|| builder.create_vector(addr.hash.as_slice()));
                    gen::Addr::create(
                        &mut builder,
                        &gen::AddrArgs {
                            hash: Some(hash),
                            start: addr.start,
                            len: addr.len,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let addrs = if addrs.is_empty() {
                None
            } else {
                Some(builder.create_vector(&addrs))
            };
            let user = entry.owner.user.as_ref().map(|user| {
                name_to_pb
                    .entry(user.to_owned())
                    .or_insert_with(|| builder.create_string(user))
                    .to_owned()
            });
            let group = entry.owner.group.as_ref().map(|group| {
                name_to_pb
                    .entry(group.to_owned())
                    .or_insert_with(|| builder.create_string(group))
                    .to_owned()
            });
            let apath = Some(builder.create_string(entry.apath()));
            let target = entry
                .target
                .as_ref()
                .map(|target| builder.create_string(target));
            let unix_mode = entry
                .unix_mode
                .as_u32()
                .map(|mode| gen::UnixMode::new(mode.try_into().expect("unix mode too large")));
            gen::Entry::create(
                &mut builder,
                &gen::EntryArgs {
                    apath,
                    addrs,
                    kind: entry.kind().into(),
                    target,
                    mtime: entry.mtime,
                    mtime_nanos: entry.mtime_nanos,
                    unix_mode: unix_mode.as_ref(),
                    user,
                    group,
                },
            )
        })
        .collect();
    let n_entries = fb_entries.len();
    let fb_entries = builder.create_vector(&fb_entries);

    let index = gen::Index::create(
        &mut builder,
        &gen::IndexArgs {
            entries: Some(fb_entries),
        },
    );
    builder.finish(index, None);

    let buf = builder.finished_data();
    let mean_size = buf.len() / n_entries;
    debug!(
        serialized_len = buf.len(),
        n_entries, mean_size, "serialized index to flatbuf"
    );
    out_file.write_all(buf)?;
    debug!("wrote to out file");
    Ok(())
}

impl From<Kind> for gen::Kind {
    fn from(value: Kind) -> Self {
        match value {
            Kind::Dir => Self::Dir,
            Kind::File => Self::File,
            Kind::Symlink => Self::Symlink,
            _ => panic!("Can't serialize kind {value:?} to flatbuffers"),
        }
    }
}
