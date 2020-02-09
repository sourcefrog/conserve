# Conserve Manifesto / Requirements

## Priorities

1. The overall priority is that stored data be retrieved when it's
   needed. Within reason it's worth compromising space, speed, or other
   metrics to increase the chance of this.

2. Time-to-restore after data loss is important, both for a full restore
   and a restore of only part of a subtree.

3. For the data to be retrieved it must first be stored, so backups must
   proceed reasonably fast and must make incremental progress.

4. Storage has a cost. The more data that is stored the longer it will take to
   read and write. With more efficient storage, users can choose to keep history
   for longer, which might let them restore the file they need.

## Feature requirements

Backup and restore trees of files, directories, and symlinks.

Preserve file mtimes.

Preserve Unix permissions, owner, and group. (Windows permissions, or Unix ACLs,
are a lower priority.)

Exclusion by path including `**` to skip multiple directories.

File names may be assumed to be valid Unicode. (On Linux, you can have byte
string file names not readable in the filesystem encoding, typically UTF-8. This
does occur but I'm not sure it's worth the complexity.)

Restore can retrieve one or more subdirectories of the tree, in time closer
to the size of the needed data than of the whole tree.

One Conserve archive backs up one source. Support for multiple concurrent
writers is not needed.

## Robustness

We will assume there will be bugs in Conserve and underlying software, and there
may be data loss in underlying storage. When problems occur, Conserve should not
fail entirely.

The program should not abort due to one missing or corrupt file, or other
errors affecting a single file, on either read or write.

The format should not be brittle: if some data is lost due to a bug or a problem
with underlying storage, it should still be possible to retrieve the rest of the
tree. This has consequences for both the format design, and how errors are
handled.

If an error occurs during backup or restore, we log it and continue. However, it
must be clear to the user that problems did occur. In particular at the
conclusion of the operation, Conserve should flag that not all files were
copied, if that's the case, so that the message is not lost in text that may
have scrolled off the screen. And, if there were problems, it should of course
have a non-zero error code.

Use simple formats and conservative internal design, to minimize the risk of
loss due to internal bugs.

The backup archive should be a pure function of the source directory
and history of backup operations.  (If the backup metadata includes
a timestamp, you can pass in the timestamp to get the same result.)

## Write-once

Files once written should never be updated, moved, or removed, unless and until
the user chooses to remove the data they contain.

Although this constrains the format it is an important property for several
reasons:

* If a later release of Conserve (or libraries it relies upon, or its compiler)
  has a bug, files written by earlier versions won't be damaged.

* If the process or machine stops unexpectedly while rewriting existing files,
  it's possible that the old file is deleted and the new file is not readable.

## Retrenchment

It is a supported mode to make a backup every day and keep them forever.

However, commonly users will want to retire or retrench some older backups,
and to keep less-frequent versions for the distant past.

Conserve will allow you to delete previous versions. Retrenchment speed should
be proportional to the data being removed, not to the total size of the archive
or tree.  Per the robustness principles, retrenchment operations should not
touch or rewrite any files other than those being deleted.

## Assumptions about backing storage

Storage to cloud object stores, local disks, and removable media are all
important. Conserve should rely on only features common across all of them.

- You can write whole files, but not update in place.

- May have relatively long per-file latency on both read and write.

- Storage bandwidth may be relatively limited relative to the source tree
  size.

- No filesystem metadata (ownership etc) can be stored directly; it must
  be encoded

- You can list directories (or, "list files starting with a certain prefix")

- May or may not be case sensitive.

- Can't detect whether an empty directory exists or not, and might not have a
  strong concept of directories, perhaps only ordered names.

- Do not assume that renaming over an existing file is allowed or disallowed.

- Conserve can cache information onto the source machine's local disk, but of
  course this cache may be lost or may need to be disabled. (We don't currently
  do this, and it would keep things simpler and more robust not to.)

- Connection may be lost and the backup terminated at any point.

- No guarantee of read-after-write consistency. (In practice, perhaps several
  seconds after writing the change will be visible.)

We cannot assume a remote smart server: the only network calls are
read, write, list, delete, etc.

Cloud storage has underlying redundancy so is unlikely to have
IO errors, although error-correcting formats may be useful on USB
storage.

Cloud storage may have multiple concurrent clients and
may not strictly serialize operations. Writes or deletes may take
some time to be visible to readers. So, locking patterns that would
work locally will not work.

Storage is private by an ACL, but still held by a third party so
should have an option for encryption.  However there is a risk the keys
will be lost so encryption should be optional.

Backing storage may be limited in what filenames it allows, eg
only case-insensitive ASCII.

Cloud storage has a cost per byte (currently ~$0.12/GB/year)
but unlimited available capacity.

## Scaling and performance

Restoring a single file or a subtree of the backup must be reasonably
fast, and must not require reading all history or the entire tree.

Listing the contents of a single directory or sub-tree should also be fast.

Conserve supports storage of large files that are partially rewritten over time,
such as databases or VM images. We don't expect data will be _inserted_ with the
rest of it being moved along, so an rsync-style sliding window is not needed.
However some ranges or blocks of the file may be overwritten.

The overall target scale is: a large single machine making backups every day, of
as much data as it can read and write in a day, keeping them all for twenty
years. Assume it has 10TB of storage in about 1e9 files, and changes 1TB of it
per day.

Coping well with many small (or even empty) files is also important.

(It's not in scope to try to back up a whole cluster of machines or distributed
filesystem as a single tree.)

## Progress

Since the source is very large and the backing storage is relatively
slow, backups must always be able to make some forward progress and
achieve some useful progress, without needing to read the whole source or
destination.

Given time to read and write (say) 100MB of source and destination,
approximately 100MB of user data should be stably stored in the
archive.

It's very possible that the size of the source relative to the IO bandwidth of
the destination means writing all the new data will take hours. This can most
easily happen on the first backup, but als on incremental backups.

In that case the backup may be interrupted - by the user interrupting it,
machine going to sleep, or losing connectivity, or rebooting.

After an interrupted backup, two properties should hold:

* Files that were already stored should be retrievable.

* When the program starts again, it should not repeat work that was already
  done, but rather start storing files that weren't already stored.

This excludes a few design options taken by other programs:

* An up-front walk or index of the whole tree is undesirable because time is
  passing with nothing being copied. In the worst situation the program will
  be interrupted before actually storing anything.

* Writing a snapshot or index of the backup only after all files have been
  stored, without which they cannot be read.

* Reading the entire content of all files from the start of the tree,
  every time the backup starts.

Remembering a save-point on the source machine seems more dangerous than  looking
in the archive to see what's been stored.


## Validation

Test restores of the whole tree take a long time and users don't do them
that often.

Conserve will provide and encourage reasonably-fast internal consistency
(_validation_) checks of
the backup that don't require reading all data back (which may be too slow
to do regularly).

Also, possibly-slow _verification_ checks that actually do compare the backup
to the source directory, to catch corruption or Conserve bugs.  These can
flag false-positive if there have been intended changes to the source
directory after the backup, so the results need to be understandable.


## Hands-off

Conserve will let you set up cron jobs to do daily backups, verification,
and retrenchment, and it should then run hands off and entirely unattended.
(Users should also do a black-box restore test, which should never fail.)


## UI

Conserve will have a human oriented text UI, and a machine UI that can
be used to implement out-of-process UIs for the web or a GUI.
