# Combining small blocks

<https://github.com/sourcefrog/conserve/issues/66>

**Status: ✅ Implemented, although some optimizations are still possible.**

## Summary

Store, within the blockdir, _combined blocks_ which compress, as a single unit,
the concatenated content of many small files. Within the file index entries,
specify the offset and length of their position within the concatenated content.

## Background

As of Conserve 0.6.2, large files are broken into multiple files, each of at
most 1 MiB uncompressed. Smaller files are written as one block each. Blocks may
be referenced by any number of index entries, across multiple bands, either by
an incremental backup noticing that the file has not changed, or simply by the
contents of the block having the same BLAKE2 hash.

In this current scheme, small files with the exact same content (across versions
or within a tree) will be matched by hash and stored only once. Large files have
a chance to match on aligned blocks. In particular large files that grow at the
end, such as logs, can match on their first MiBs and then store separate blocks
for their fractional MiB tail.

Previous designs contemplate, and the current index format leaves a space for,
instead having blocks that store content from multiple files, at different
offsets.

## Motivation

In the current scheme, trees with many small files produce many small blocks.

Source trees are a good example of this. For example in an arbitrary built Rust
tree that I have here, there are only 263 files over 1MiB, and 44876 under 1MiB,
of which 40210 are under 10kiB, and 30756 under 1kiB.

This is not terrible, but there are some disadvantages to producing many small
blocks.

Reading and writing a file has some latency (and CPU and IO bandwidth) overhead,
and this may become a limiting factor on files with many small trees.

The latency impact can, perhaps, be reduced by reading and writing many files in
parallel, which we'll want to do anyhow, and which Conserve's general design can
support. But we are still making the OS do more work.

Filesystems, storage devices, typically have a fixed block size of typically
4kiB. All files under 4kiB will typically end up using a full block, which means
there's actually fairly poor compression on such trees, if measured in terms of
archive disk space consumed.

## Design sketch

Define some sizes, such as: `BLOCK_SIZE = 1MiB`, `SMALL = 100kiB`. Files smaller
than `SMALL` will be written into _combined blocks_ rather than individual
blocks. (Perhaps 100kiB is too high, and the limit should be more like 10kiB?)

### Backup

To make a backup, collect a set of entries (say 1000) to make the next index
hunk. Anything larger than `SMALL` is compressed into a set of independent
blocks, as in Conserve 0.6.2.

The total size of the small files is at most 1000 entries \* 100kiB, or ~100MB.
(This is the worst case, where all the entries are the largest possible small
files, and all are new: typically there will be some directories or larger
files, or some small files unchanged from the previous tree, or small files much
less than 100kiB. But source may approach this case.)

For an incremental backup, small files are treated the same as large files and
the same as previously: if their size and mtime is the same as in the basis
tree, the addresses are copied across, and there's no more to be done.

Zero-size files also can be immediately special cased as a file entry with no
addresses, and need not be considered any further.

If these variables change such that the concatenation of small files is too
large to be a single block, then we could accumulate them into several combined
blocks per index hunk.

In parallel, read the contents of all of these small files into one buffer each.
Remember the lengths of the per-file buffers. Then, feed them one after the
other into both a hasher and a compressor, producing a hash and compressed form
of the concatenation of all of the buffers. This is the new _combined block_.
Write this into the block dir.

Now that its hash is known, generate index entries for the small files,
including the hash of the combined block, and their index and length within it.

The combined block finishes before the index hunk for its files.

In this approach, the tail of a long file (modulo `BLOCK_SIZE`) is not treated
as a small file, but rather stored as its own block. So, there may still be some
small blocks, but these are presumably rare.

### Restore

During restore operations, we must take care not to read and decompress the
combined block once for every file that references it. Since the combined block
runs for only the span of files in an index hunk, we can see by looking at the
addresses in that hunks entries which block/s are needed by multiple files. (In
fact, this is perhaps useful even for regular, long, blocks, if there are
identical files inside the same hunk.) The decompressed contents of them can be
held in memory and data may be picked out of it by multiple entries.

## Consequences

### Small files can't match by hash

Since the combined blocks are a concatenation of potentially many files that
happen to be in the same index hunk, it's unlikely the same hash will ever
arrive again. And, there is no direct way to discover that multiple small files
are identical.

### Fragmentation across combined blocks

After many incremental backups of a tree of many small files, the last index may
refer to combined blocks from many different prior backups.

Restoring from such a tree may require reading and decompressing a relatively
large volume of combined blocks, just to pick out a small amount of desired data
from each.

Also, when expiry is added, these blocks can't be deleted until no later version
refers to them, so they may hang around for a long time.

Therefore, `restore` may be slower for archives with long histories of small
files. And expiry may free up relatively less space: in the worst case, it will
free up no space at all, if there is just one file still present in the last
tree, from all previous blocks.

### Larger IOs and less file manipulation overhead

Per byte stored, we'd expect to dispatch fewer system calls and fewer IOs, and
so throughput during backup would be higher.

### Better dictionary compression

If many small files, perhaps text files, are written inside one block, there's
an opportunity for Snappy (or other future block compression) to make use of
repeated patterns across files, which isn't possible if they're stored
separately.

This would reinforce the space savings that come from avoiding smaller blocks.

### Format invariants and validation

As before, all blocks referenced by the index must be in the blockdir. Blocks
must be written before the index hunk referencing them is written, which the
algorithm sketched above achieves.

When checking index entries we must check that their addr's `start + length` is
less than the uncompressed length of the block.

This doesn't require a new major format version, but it does create index
entries that older versions won't be able to read. (Possibly this should be
handled more gracefully by marking each band header with the minimum version to
read it, as in #96 and [this RFC](band-version-headers.md).)

### Complexity

The system, with this feature, is more complex than without it. The changes to
the data format are modest, but the changes to the algorithms to read and write
it with good performance are somewhat significant.

## Alternatives

### Periodically write new blocks to enable GC

Perhaps we could have an option to `backup` telling it not to refer to old
combined blocks, to allow them to eventually be expired.

Is there any way to automatically infer this should be done? We don't directly
know how old a block is.

### Repack during gc

I would generally rather avoid this, but we could potentially rewrite both
blocks and the index during gc to allow old blocks to be removed.

(Indeed if doing this there is the potential for much larger compression into
something like git pack files.)

### Store combined blocks separately?

I'm not sure of any specific benefit from keeping them separate.
