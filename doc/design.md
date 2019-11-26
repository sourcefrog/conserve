Conserve design
===========

Conserve is a backup system for Unix systems in today's world: especially, for
backing up to either nearby local disks or in to cloud storage.

Storage environment
-------------------

Conserve makes limited assumptions about the archive storage, guided by what is
commonly supported by cloud storage.

- You can write whole files, but not update in place

- Almost unlimited in size

- Very long round trips

- Fairly limited bandwidth relative to the amount of data to be
  transmitted.

- No filesystem metadata (ownership etc) can be stored directly; it must
  be encoded

- You can list directories (or, "list files starting with a certain prefix")

- May or may not be case sensitive.

- Can't detect whether an empty directory exists or not, and might not have a strong
  concept of directories, perhaps only ordered names.

- Can try to not overwrite files, but not guaranteed coherent.

- Conserve can cache information onto the source machine's local disk, but of course
  this cache may be lost or may need to be disabled.

- Connection may be lost and the backup terminated at any point.

- No guarantee of read-after-write consistency.  (In practice, perhaps several seconds
  after writing the change will be visible.)

Requirements
------------

- Data should still be recoverable even if there are bugs in this program,
  or in other systems.

- The formats should be very simple and if possible recoverable by hand.

- Layers for Reed-Solomon encoding, encryption (plain gpg), compression

- Fast random access to retrieve particular files or directories

- Fast listing of what's in a particular layer of the archive

- Backups ought to make incremental progress, even if they can never
  complete a whole band in one run.


Testing
-------

- Effort testing

- Side-effect free state machines for core algorithms so they can be
  tested without doing real large work


Write/write concurrency
-----------------------

Conserve is supposed to be run with just one process writing to destination archive at
any time, obviously just from one source.  It is basically up to the user to
configure the clients so this happens: to make sure that only one logical machine
tries to write to one archive, and that only one backup process runs on that machine
at any time.

However, it is possible something will go wrong and we end up with an inadvertent
dual-master situation.  Requirements for that case are:

 - Conserve should not damage the repository.

 - If possible, one writer should continue and write a full valid backup,
   and all the others should terminate.

 - This situation is expected to be rare so detecting it should not impose a large
   performance or complexity cost.

There may be different cases depending when the race occurs:

 - both are starting a new stripe

 - both writing blocks within an existing strip

Possible approaches (not mutually exclusive):

 1. Write a lock file when active and remove it when done.  This is difficult
    because of confusion about taking the lock without global consistency,
    and the client may die holding its lock.

 2. Name block files uniquely so that multiple writers don't conflict.  Not great
    though because effort and space will be wasted making multiple backups.

 3. Use deterministic names and detect if the file to be written already exists
    (maybe writing it using a do-not-overwrite option, if the back end supports
    that).  But, without global consistency, we're not guaranteed to detect
    conflicts.

 4. Check the most-recently-written file before starting.  If it's recent
    (within say 20 minutes) and not from the same machine, or not from a
    process we can see is dead, warn or pause or abort.

 5. Keep a client-side lock file, on a filesystem that probably is coherent.
    Store the pid and similar information to try to detect stale processes.
    (Doesn't protect against multiple machines all thinking they should
    write to the same archive, or eg having different home directories on the
    same source.)

 6. Just make sure each block can be read in isolation even they do come
    from racing processes - minimal data dependencies between blocks.


Read/write concurrency
----------------------

Logical readers are physically read-only, so any number can run without interfering
with writers or with each other.

Because the storage layer does not promise coherency readers will see data in
approximately but not exactly the order it was written by the writer.  In practice
this means that some files we might expect to be present may be missing.

Perhaps, if a file is missing, we should wait a few seconds to see if it appears.

But, the file may be permanently missing, perhaps because the writer crashed
before the file was committed.

So concurrency seems to be just a special case of readers being robust with
incomplete or damaged archives.


Verification
------------

- Check the contents of each block against hashes stored in the archive.

- Check across different archives of the same source: in particular, is
  what's stored in the cloud the same as what's in the local cache?
  Even if the local cache only contains summary information not block
  data?

- Make sure all files can be read out with the intended hash.


Block size tradeoffs
--------------------

We have to do one roundtrip to the archive per block, so we don't want
them to be too small.  It might be worse than one (if there's also a
header file per block).

I think it's good not to split files across blocks - but this does mean
that blocks can grow arbitrarily large if you have large files.

Backup
------

Backup is essentially: walk over the source tree, subject to exclusions. Trees
are always walked in apath order.

For each file, hash every block, and store all their blocks.

Build an index including references to these blocks.

Incremental backups
-------------------

(not implmented yet)

When there's a previous backup, we can avoid some work of hashing blocks, by
looking at the previous index. If the file has the same mtime/ctime and length
as in the previous tree, we can assume it has the same content, and just copy
across the block hashes from the previous version. We should also check that
those blocks do actually exist.

The parallel-iteration code is similar to, or builds on, what is needed to
implement diff.

Continuing interrupted backups
------------------------------

(not implmented yet)

To continue with a band, we need to just find the last file completely
stored, which is the last name of the last block footer present in this
bound.

It might also be worth checking that all the data blocks for the interrupted
backup have actually been stored.

Random features
---------------

- How can we avoid every user needing to manually configure what to
  exclude?  Perhaps the OS can ship suggested exclusion lists that match the
  apps?

- Exclude files from future backups but don't mark them as deleted

- Eventually, rdiff compression only on large files

- Prioritize some important files, rather than working in filesystem
  order?  Or maybe have a top tier that's masked to include only some
  important files.

- Stage blocks to go to the server locally; pipeline uploads.  Eventually,
  completely pipelined everything: write all backups to local disk (if you
  have space and configure it that way) and then move them up to the cloud
  in the background.


UI
--

Goals:

 * accumulate all actions so they can easily be compared to expected
   results at the end of a test

 * show them in nicely formatted text output, eg with indenting,
   color or tabulation, not just log output

 * stream output rather than waiting for the whole command to finish

 * perhaps later support a gui

 * ui interactions can be externalized onto pipes

 * show progress bars, which implies knowing when an operation starts
   and ends and if possible how many items are to be processed

 * simple inside the main application code

 * not too many special cases in the ui code

Emit fairly abstracted events that can be mapped into a ui, or just
emitted to stdout.  Maybe emit them as (ascii?) protobufs?

Human strings are internationalized: this should be done strictly in
the UI layer.  Debug/log strings can be emitted anywhere and don't need
i18n.

XXX: is it enough, perhaps, just to use logging? Perhaps that's the
simplest thing that would work, for now, enough to do some testing?
Open questions:

 * Transmit the actual text to be shown to the user, or some kind of
   symbol?  Text is enough to test it, but not so good for reformatting
   things.


Alarms
------

When Conserve hits something unexpected in the environment, the core code will
signal an *alarm*, and then attempt to continue.  The alarms are structured
and can be filtered.  The default handlers will try to balance safety
vs completion, but they can be customized.  In particular, you can tell
it to accept everything and try hard to continue, so you have the best chance
of recovering something from a damaged backup.

This is somewhat similar to Python's warnings module, but a different
implementation, because Python is so tied to the warnings being about code.

Fields:

 - *area*: source, archive, band, block, restore
 - *condition*:
   - missing
   - ioerror
   - denied: permissions error from the OS
   - corrupt: protobuf deserialization failed, etc
   - mismatch: hash is not what a higher-level object says it should be
   - exists: a file to be written already exists
 - *filename*
 - *message* - only what can't be stored elsewhere

Handling options:
 - abort
 - continue (with a warning)
 - ask (interactively; perhaps not very useful in a long backup)
 - suppress (with only a debug message)

The default should probably (?) be to abort on almost everything, except perhaps
not on source alarms.

It should be possible to get a summary, and machine-readable details of alarms
fired.


Return codes and result reporting
---------------------------------

It's bad if a backup aborts without storing anything because of a footling
error: it may be some unimportant source file was unreadable and therefore
nothing was stored.  On the other hand, it's also bad if the backup apparently
succeeds when there are errors, because the file that was skipped might have
actually been the most important one.

Therefore there need to be concise and clear summary results, that can be
read by humans and by scripts reading the output, and an overall one-byte
summary in the return code.

Possible return codes:

 - everything was ok (0)
   - no alarms at all
   - backup completed

 - backup completed with warnings:
   - some source files couldn't be read?
   - every source file that could be read has been stored

 - backup completed but with major warnings:
   - some already stored data seems to be corrupt?

 - fatal error
   - bad arguments, etc
   - unexpected exception

We also need to consider the diff, verify, validate cases:

 - some data is wrong or missing, but it may still be possible to restore
   everything (eg a hash is wrong)
 - some data is wrong or missing so at least some files can't be restored
 - the source differs from the backup, in ways that might be accounted for by
   changes since the date of the backup
 - the source differs from the backup, with those changes apparently dating
   from before the backup was made
