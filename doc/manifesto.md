# Manifesto / Requirements

## Priorities

1. The overall priority is that stored data be retrieved when it's
   needed, and within reason it's worth compromising space, speed, or other
   metrics to increase the chance of this.

2. Time-to-restore after data loss is important, both for a full restore
   and a restore of only part of a subtree.

3. For the data to be retrieved it must first be stored, so backups must
   proceed reasonably fast and must make incremental progress.

4. Storage has a cost, and the more data that is stored the longer it will
   take to read and write.


## Robustness

There will be bugs in Conserve and underlying software, and there may be data
loss in underlying storage. When problems occur, Conserve should not fail
entirely.

The program should not abort due to one missing or corrupt file.

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

Files once written should never be updated until their data is retrenched.


## Retrenchment

It is a supported mode to make a backup every day and keep them forever.

However, commonly users will want to retire or retrench some older backups,
and to keep less-frequent versions for the distant past.

Conserve will allow you to delete previous versions. Retrenchment speed should
be proportional to the data being removed, not to the total size of the archive
or tree.  Per the robustness principles, retrenchment operations should not
touch or rewrite any files other than those being deleted.

## Assumptions about backing storage

Storage to cloud object stores, local disks, and removable media are
all important.

### Cloud object stores

Cloud storage (and to a lesser extent local network or USB storage)
is high-latency and limited bandwidth.

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



## Scaling

Restoring a single file or a subtree of the backup must be reasonably
fast, and must not require reading all history or the entire tree.

Conserve supports storage of large files that are partially rewritten over
time, such as databases or VM images. We don't expect data will be
_inserted_ with the rest of it being moved along, so an rsync-style
sliding window is
not needed. However some ranges or blocks of the file may be overwritten.

The overall target scale is: a large single machine making backups every day,
of as much data as it can read and write in a day, keeping them
all for twenty years.  So on the order of: 10-100TB in about 1e9 files.

(It's not in scope to try to back up a whole cluster of machines or distributed
filesystem as a single tree.)



## Progress

Since the source is very large and the backing storage is relatively
slow, backups must always be able to make some forward progress and
achieve some goodput without needing to read the whole source or
destination.

Given time to read and write (say) 100MB of source and destination,
approximately 100MB of user data should be stably stored in the
archive.

The program should expect to be regularly interrupted (by eg being
stopped or losing connectivity) and should cleanly resume when re-run.


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
