# Design ideas

(This doc contains things that are too vague or too early to be in the issue
tracker, or a broader discussion of things that are there.)

## Readahead

When reading from either a local tree or a stored index, we could read-ahead to
overlap IO, decompression, and deserialization. (This isn't obviously a big
performance driver at the moment, though.)

I think we could have a generic `ReadAheadIterator` that takes an iterator that
is `Send` or similar, and returns items that are `Send`. Then just push them
into a synchronous channel of given capacity.

## Delta indexes

Incremental backups still write a full copy of the index, listing all the
entries in the current tree. This in practice seems to work pretty reasonably,
with an index only about 1/1000th the size of the tree. (For each file there's
about 100 bytes in the name and block references.)

(I used to think this would be very important, but experience seems to show it's
not so much.)

We could add a concept of higher-tier versions, that record only files stored
since a basis index.

1. An index concept of a whiteout.

1. A tree reader that reads several indexes in parallel and merges them.
   (Something much like this will be needed to read incomplete trees.)

1. A tree writer that notices only the differences versus the parent tree, and
   records them, including whiteouts.

It seems like we'd need some heuristic for when to make a delta rather than full
index. One possibility is to look at the length of the previous delta index: if
it's getting too long (perhaps 1/4 of the full index?) then just store a full
index.

## Validate

Validation checks some invariants of the format, to catch either bugs or issues
originating in the environment, like disk corruption.

Perhaps more of the work here is in creating tests that make variously broken
archives and validate them - positive cases for validation.

What bugs are actually plausible? What failures could be caused by interruption
or machine crash or other likely underlying failures?

How much is this similar to just doing a restore and throwing away the results?

- For the archive
  - [done] No unexpected directories or files
  - [done] All band directories are in the canonical format
- For every band
  - The index block numbers are contiguous and correctly formated
  - No unexpected files or directories
- For every entry in the index:
  - Filenames are in order (and without duplicates)
  - Filenames don't contain `/` or `.` or `..`
  - The referenced blocks exist
  - (Deep only) The blocks can be extracted and they reconstitute the expected
    hash
- For the blockdir:
  - No unexpected top-level files or directories
  - Every prefix subdirectory is a hex prefix of the right length
  - Every file inside a prefix subdirectory matches the prefix
  - There are no unexpected files or directories inside prefix subdirectories
  - No zero-byte files
  - No temporary files
- For every block in the blockdir:
  - [done] The hash of the block is what the name says.
  - All blocks are referenced by one index

Should report on (and gc could clean up) any old leftover tmp files.

## gc leftover blocks

Unreferenced blocks should be rare, but can happen if a backup is interrupted.

Blocks can be garbage collected (only) by reading the list of all present
blocks, and then subtracting the ones referenced by any band's index. Anything
left unreferenced can be deleted. It's important the operation be done in this
order, so that it's safe in the case a band is being written concurrently with
the gc operation, or if the filesystem is not quite coherent.

## Better ignore patterns

- `--ignore .git` should probably ignore that anywhere in the tree. At present
  it'll try and fail to match the whole path.

  Perhaps this should be the same as gitignore
  <https://git-scm.com/docs/gitignore>.

  This might require a change from <https://docs.rs/globset/0.4.2/globset/> that
  we use at present.

* How can we avoid every user needing to manually configure what to exclude?
  Perhaps the OS can ship suggested exclusion lists that match the apps?

## Diff backup against source

It'd be really nice to account for which changes might have been due to later
changes in the source:

- Files are different and the file in the source is newer.

- File is missing from the source and potentially deleted since the backup was
  created.

Perhaps run a named command (like `diff`) on files that differ.

## Performance

- Use minimal compression on files whose name indicates they're already likely
  to be compressed (jpg, mpg, mp3, gz, etc)

- Try <https://github.com/TyOverby/flame> flamegraph profiling. (May not be
  useful if the compression/hashing/etc is very tightly interleaved? But we can
  still try.)

## Store/restore metadata

- mtime
- x-bit
- permissions, owner, group - maybe shouldn't be on by default?

Being unable to set the group or owner should be a problem that's by default
only a warning.

## Backup with `O_NOATIME`?

## Partial restore

Restore only a named subdirectory or file.

This should also restore the parent directories with the right permissions, but
also not complain if they already exist.

## Restore incomplete band

If asked to restore an incomplete band, act analogously to resuming a backup:

- Restore to the end of the index of the incomplete band.
- Give a warning.
- Seek in the previous index to just after the last successfully read file, and
  continue restoring from there.
- Repeat this as long as necessary until you reach the end of a complete band.

This'd be nice to have but it tends to bias towards having freshest copies of
the alphabetically first files, which is not so great. Before doing this we
should resume interrupted backups, to avoid that effect.

After doing this, it's safe for `restore` to choose the most recent band even if
it's incomplete. Similarly `ls` etc.

## Robustness

- Test handling of various broken archives - perhaps needs some scripts or
  infrastructure to construct them
- decompression failure
- missing block
- bad block
- missing index file
- File is removed during reading of index

## Testing

- Add more unit tests for restore.
- Interesting Unicode names? (What's interesting?)
- Filenames that cause trouble across Windows/Unix.
- Test performance of block storage by looking at counts: semi-white-box test of
  side effects
- Filesystem wrapper to allow injecting faults
- Detection of corrupt block:
  - Wrong hash
  - Decompression fails
- Helper to compare trees and show diff
- Helper for blackbox tests: show all output if something fails in the test. (Is
  it enough to just print output unconditionally?)
- Rename `testsupport` to a seperable `treebuilder`?

## Resume interrupted backup

- Detect there's an interrupted band
- Look at what index blocks are already present
- Find the last stored name from the last stored index block
- Maybe check all the data blocks from the last index block are actually stored,
  to know that the interruption was safe?
- Resume from that filename

## Parallelism

Both reading and writing do a lot of CPU-intensive hashing and de/compression,
and are fairly easy to parallel.

Parallelizing within a single file is probably possible, but doing random IO
within the file will be complicated, especially for non-local filesystems.
Similarly entries must be written into the index in order: they could arrive a
bit out of order but we do need to finish one chunk at a time.

However it should be easy to parallelize across multiple files, and index chunks
give an obvious granularity for doing this:

- Read a thousand filenames.
- Compress and store all of them, generating index entries in the right order.
  (Or, sort the index entries if necessary.)
- Write out the index chunk and move to the next.

It seems like it'll fit naturally on Rayon, which is great.

I do want to also combine small blocks together, which means the index entry
isn't available immediately after the file is written in, only when the chunk is
complete. This could potentially be on a per-thread basis.

## Backup multiple source directories

It seems like `conserve backup ARCHIVE ~/src/conserve ~/src/conserve.wiki` ought
to work, and create a similar result to as if we backed up `~/src` containing
only those two subdirectories.

However this introduces several hairier cases:

- What if they're not in the same parent directory?
- What if some have the same last name component?

Perhaps this is best considered as sugar for: backup the tree starting at their
common ancestor, but exclude everything other than the named directories.

Doing so would mean that adding another directory with a different common
ancestor, would case everything to move.

Perhaps there should be an option for the base directory.

## Security

- Salt the hashes to avoid DoS collision attacks, and to enable encryption.
  (Store the salt in the base tier? Requires version bump.)
- Asymmetric encryption? Perhaps better to rely on the underlying storage?
- Signing?

## Cloud storage

- VFS abstraction
  - Make this a separate Rust package?
- `conserve replicate` to copy bands from an archive without changing the
  content?
  - Like an ordering-aware `gsutil rsync` or `rsync`
- Test on GCS FUSE
- For remote or slow storage, keep a local cache of which blocks are present?

## Questionable features

- Store inode numbers and attempt to restore hard links
- Store file types other than file/dir/symlink

* Exclude files from future backups but don't mark them as deleted
