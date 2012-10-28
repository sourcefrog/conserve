Dura design
===========

Dura is a backup system for Unix systems in today's world: especially, for
backing up to either nearby local disks or in to cloud storage.

Storage environment
-------------------

- You can write whole files, but not update in place

- Almost unlimited in size

- Very long round trips

- Fairly limited bandwidth relative to the amount of data to be
  transmitted.

- No filesystem metadata (ownership etc) can be stored directly; it must
  be encoded

- You can list directories (or, "list files starting with a certain prefix")

- May or may not be case sensitive

- Can't detect whether an empty directory exists or not

- Can try to not overwrite files, but not guaranteed coherent

- There is some prospect of local caching (but, relying on caches being
  consistent might be dangerous or make behaviour unpredictable)

- Connection may be lost and the backup terminated at any point.

- Not absolutely guaranteed read/write coherent (but Google Cloud Storage
  is?)

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


Open questions
--------------

- Locking on the repository against multiple concurrent writers?  Or, can
  we avoid that by just having each writer choose their own names for each
  layer/block etc.

- What if two processes are trying to continue with the same band at the
  same time, and they both try to write similarly-named blocks?  Perhaps
  it's ok to say the storage layer can detect this case and will abort
  when it notices the block has appeared.

- Maybe not actually tar?  Robert points out there's a lot of historical
  cruft in the format, the format is not well defined, and there might be
  some waste.


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


Incremental update
------------------

There are two approaches to doing an incremental update: go just by date,
or look at every file relative to the previous bands.  The former might
accidentally upload some files repeatedly; the latter might require
rereading all the previous indexes if we don't already have them cached.

To continue with a band, we need to just find the last file completely
stored, which is the last name of the last block footer present in this
bound.


Concurrency
-----------

Concurrency is a bit hard because the storage layer is not necessarily
coherent.

Mostly have to assume people will, out of scope, make sure not to run two
backups to the same archive at the same time.  Possibly could create a
lock file against this on the source machine?

Possibly can just abort when an unexpected file already exists.

Let's just not worry about it for now.


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

Emit fairly abstracted events that can be mapped into a ui, or just
emitted to stdout.  Maybe emit them as (ascii?) protobufs?

Human strings are internationalized: this should be done strictly in
the UI layer.  Debug/log strings can be emitted anywhere and don't need
i18n.


Alarms
------

When Dura hits something unexpected in the environment, the core code will
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
 - *filename*
 - *message* - only what can't be stored elsewhere

Handling options:
 - abort
 - continue (with a warning)
 - ask
 - suppress (with only a debug message)