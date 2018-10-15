# Conserve TODO

## gc

Read all blocks that are present. Read all blocks that are referenced.
Delete blocks that aren't referenced.

## `archive size`

`conserve versions --sizes` won't say much useful about the version sizes
any more, because most of the disk usage isn't in the band directory.
Maybe we need a `conserve archive describe` or `conserve archive measure`.

We could say the total size of all blocks referenced by that version.

Perhaps it'd be good to say how many blocks are used by that version and
not any newer version.

## Incremental indexes

1. An index concept of a whiteout.

1. A tree reader that reads several indexes in parallel and merges them.

1. A tree writer that notices only the differences versus the parent tree,
   and records them, including whiteouts.

## Validate

Before too long, should add a concept of validation that checks invariants
of the format. Perhaps this's worth having in place before doing incremental
backups because they'll complicate the format.

Perhaps more of the work here is in creating tests that make variously
broken archives and validate them - positive cases for validation.

What bugs are actually plausible? What failures could be caused by interruption
or machine crash or other likely underlying failures?

How much is this similar to just doing a restore and throwing away the results?

* no unexpected files or subdirectories, in any directories
* hash of data files is as expected
* all referenced blocks, exist

Should report on (and gc could clean up) any old leftover tmp files.

## Better ignore patterns

* `--ignore .git` should probably ignore that anywhere in the tree. At present
  it'll try and fail to match the whole path.

  Perhaps this should be the same as gitignore <https://git-scm.com/docs/gitignore>.

  This might require a change from <https://docs.rs/globset/0.4.2/globset/> that
  we use at present.

## Error handling

Clean message, and test for it, when the archive directory just doesn't exist.

## Internal cleanups

* Refactor text formatting into being part of the UI rather than within the CLI?

* Add a 'high-level' module similar to the CLI, but not coupled to it?

* Maybe don't use `error_chain`? I have an unjustified feeling it slows down
  compilation. Perhaps use `Failure`.

* Report warnings by failures/errors that are passed to the UI rather than
  returned.

## Purge old versions

Pretty easy, just delete the subdirectory. Does require checking there are no
children - or optionally delete all the children.

1. Delete incomplete old versions.

1. Delete specifically named old versions.

## Auto-prune according to some retention policy

How is the policy defined? Maximum age (per level?) Maximum number of versions
per level? Remove excessively frequent backups?

## Diff backup against source

It'd be really nice to account for which changes might have been due to later
changes in the source:

* Files are different and the file in the source is newer.

* File is missing from the source and potentially deleted since the backup
  was created.

Perhaps run a named command (like `diff`) on files that differ.

## Refactor of the UI

I'm not sure there's a really meaningful difference between the `color` and
`plain` UIs: both are essentially text UIs.  Perhaps the user visible options
should be `--color=auto` and `--progress=auto`, and it's all the same text
UI. It can contain a progress bar, or a progress sink that does nothing if
it's turned off or the terminal doesn't support it. And similarly for drawing
colors if possible and wanted, and not otherwise.

## Better progress bar

**Fix progress and logging for restore.**  Make it work the same way for restore
as backup: don't rely on any backup-specific counters.

Ideally should show some of these:

* Percent complete
* Expected time to run
* Bytes read (uncompressed source) and written (compressed and after deduplication)
* Current filename
* Progress within the current file

* Show current filename: maybe 2-line output?
* Disable progress for operations like `list-bands` that will use stdout
  for bulk data.
* Erase at end of program and replace with a summary.

Progress for `ls` is also bad.

## Better summary at end of run

Just better formatting?

* Number of files included, unchanged, stored.
* Bytes read, stored.
* Time
* `getrusage` or similar

Make it more concise and then show it by default.

## Performance

* Use minimal compression on files whose name indicates they're already
  likely to be compressed (jpg, mpg, mp3, gz, etc)
* Try <https://github.com/TyOverby/flame> flamegraph profiling.
  (May not be useful if the compression/hashing/etc is very tightly
  interleaved?  But we can still try.)
* Don't load whole data files into memory, just one block at a time.

## Problem reporting infrastructure

* Report problems
* Change log/error statements to use `report`
* Add `keep_going` option?
  * Some errors are recoverable (or are warnings) and some are not.
* Make a macro like `try!` that logs when it sees an error?
* Errors to stderr rather than stdout?
      Hard to reconcile with use of terminal for colored errors.
* Maybe have Conserve-specific error types rather than `io::Error` everywhere?

## Store/restore metadata

* mtime
* x-bit
* permissions, owner, group - maybe shouldn't be on by default?

Backup with `O_NOATIME`?

Being unable to set the group or owner should be a problem that's by default
only a warning.

## Partial restore

Restore only a named subdirectory or file.

This should also restore the parent directories with the right permissions, but
also not complain if they already exist.

## Split across blocks

* Store block hash, start, length, as distinct from the file's own hash
* Insert block-splitting layer

## Robustness

* Test handling of various broken archives - perhaps needs some scripts
  or infrastructure to construct them
* decompression failure
* missing block
* bad block
* missing index file
* File is removed during reading of index

## Testing

* Add more unit tests for restore.
* Interesting Unicode names? (What's interesting?)
* Filenames that cause trouble across Windows/Unix.
* Test performance of block storage by looking at counts: semi-white-box
  test of side effects
* Filesystem wrapper to allow injecting faults
* Detection of corrupt block:
  * Wrong hash
  * Decompression fails
* Helper to compare trees and show diff
* Helper for blackbox tests: show all output if something fails in the test.
      (Is it enough to just print output unconditionally?)
* Rename `testsupport` to a seperable `treebuilder`?

## Resume interrupted backup

* Detect there's an interrupted band
* Look at what index blocks are already present
* Find the last stored name from the last stored index block
* Maybe check all the data blocks from the last index block are actually stored,
  to know that the interruption was safe?
* Resume from that filename

## Locking

* client-side lock to prevent concurrent updates to the same store?
* Lock file in the archive, maybe in the band header?
      Won't work well on cloud storage.

## Parallelism

All of these need a bounded number of worker threads, and to run a bit ahead
of the task but to still wait for it to complete.

* Do compression on worker pool.
* Write out on a background thread

## Backup multiple source directories

It seems like `conserve backup ARCHIVE ~/src/conserve ~/src/conserve.wiki`
ought to work, and create a similar result to as if we backed up `~/src`
containing only those two subdirectories.

However this introduces several hairier cases:

* What if they're not in the same parent directory?
* What if some have the same last name component?

Perhaps this is best considered as sugar for:
backup the tree starting at their common
ancestor, but exclude everything other than the named directories.

Doing so would mean that adding another directory with a different common
ancestor, would case everything to move.

Perhaps there should be an option for the base directory.

## Security

* Salt the hashes to avoid DoS collision attacks, and to enable encryption.
  (Store the salt in the base tier? Requires version bump.)
* Asymmetric encryption? Perhaps better to rely on the underlying storage?
* Signing?

## Cloud storage

* VFS abstraction
  * Make this a separate Rust package?
* `conserve replicate` to copy bands from an archive without changing the content?
  * Like an ordering-aware `gsutil rsync` or `rsync`
* Test on GCS FUSE
* For remote or slow storage, keep a local cache of which blocks are present?

## Questionable features

* Store inode numbers and attempt to restore hard links
* Store file types other than file/dir/symlink
