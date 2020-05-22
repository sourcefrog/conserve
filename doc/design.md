# Conserve design

Conserve's design is largely shaped by its
[manifesto and requirements](manifesto.md) and [archive format](format.md), so
read those first.

## Backup

Backup is essentially: walk over the source tree, subject to exclusions. Trees
are always walked in apath order.

For each file, hash every block, and store all their blocks.

Build an index including references to these blocks.

### Incremental backups

When there's a previous backup, we can avoid some work of hashing blocks, by
looking at the previous index.

As for a non-incremental backup, we need to walk the tree in apath order, which
is the same order they're stored into the index. We can also read the previous
index in lock step.

If the file has the same mtime and length as in the previous tree, we can infer
that it has the same content, and just copy across the block hashes from the
previous version.

We also check that those blocks do actually exist: they always should if they're
referenced by the previous index, but if they don't then they'll be rewritten,
using the file content.

## Validation

`conserve validate ARCHIVE` checks various invariants of the archive
format. As for other operations, if errors are found they are logged,
but the process continues rather than stopping at the first problem.

Validation is intended to catch both Conserve bugs, and problems in
lower level systems such as disk corruption.

Properties that are checked at present include:

* There are no extraneous files in the archive directories.
* Every data block can be decompressed.
* The hash of the decompressed data in each block matches its filename.
* Every index hunk can be decompressed and deserialized.
* Index hunk numbers are consecutive.
* Files in the index are ordered correctly.
* The blocks, and region within the block, referenced by the index is present.

## Testing

Conserve has typical Rust unit tests within each source file, and then two
types of higher-level tests.

`blackbox.rs` tests the command-line interface by running it as a subprocess.

`integration.rs` tests the library through its API. (The distinction between
this and the unit tests is a bit blurry, and perhaps this should be removed.)

Various Conserve components accumulate counters of how much work they've done,
into the `Report`, and this is used to make assertions about, for example,
how many files are read to do some task.

## External concurrency

Conserve does not have an explicit lock file on either the client or
the server. Instead, the format is safe to read while it is being written.
Multiple concurrent writers are not recommended because the safety of
this scenario depends on the backing filesystem, but it should generally
be safe.

Conserve's basic approach of writing files once and never deleting them
makes this practical.

The absence of a lock file gives some advantages. Stale lock files are likely ts
be left behind if the program (or machine) abruptly stops, and detecting if
they can safely be broken is difficult. Asking the user is not a good solution
for scheduled backups, and even if a user is present they may not make the
decision reliably correctly. Finally, filesystems with weak ordering
guarantees, where concurrent writers are most complex, also make it hard to
implement a lock file.

### Write/write concurrency

Conserve is intended to be run with only one task writing to the archive at
a time, and it relies on the user to ensure this. (Typically it will be
run manually or from a cron job to back up one machine periodically.)

However, if there ever are two simultaneous tasks, this must be safe.

At present there is only one command that changes an archive, and that is
`backup`. `backup` writes a new index band and (almost always) writes new
index blocks.

When starting a backup, Conserve chooses a new band number, one greater than
what already exists, then creates that directory and writes into it. There is
conceivably a race here, where two writers choose the same band. Depending on
the filesystem behavior, they should notice the band has already been created,
and abort.

Index blocks are written by atomically renaming them in to place. If the block
already exists, the new version (with identical contents) is simpy discarded.
So, concurrent writes of blocks are safe, and indeed can happen from multiple
threads in the same process.

When expiry or purge commands are added they'll also need care.

An active backup writer can potentially be detected by looking for recent
bands or index hunks, but this is not perfect.

### Read/write concurrency

Conceivably, one task could try to restore from the archive while another
is writing to it, although this sounds contrived.

Logical readers are physically read-only, so any number can run without
interfering with writers or with each other.

Because we don't assume perfectly consistent read-after-write ordering
from the storage, it's possible that readers see index hunks before
their data blocks are visible. This will give an error about that file's
content being missing, but the restore can continue.

The reader will observe an incomplete index, and this is handled just as if
the backup had been interrupted and remained incomplete: the reader
picks up at the same point in the previous index. (This last part is not
yet implemented, though.)

## UI, Progress, and Logging
Conserve's user interface includes:

* Initially parsing the command line and showing errors or help. This is fairly
  simple and occurs before anything else, so need not be discussed any further.

* Showing log messages and errors.

* Showing progress bars or other indications.

* Emitting content to stdout, such as file listings or file content.

* (Later) Reading encryption passphrases. Or, potentially, getting other input
  or confirmation from the user, but this will be limited.

The library should support several modes of UI:

1. Primarily, the text UI presented by the `conserve` binary, on a terminal that
   allows cursor control.

   In this case the terminal is inherently a global singleton across the
   process, and all the different uses need to be coordinated. Most importantly,
   log output must interleave with progress bars.

2. Noninteractive text output, when there is no terminal. This should be similar
   to the terminal, but with progress bars and interactive input turned off.

3. Other applications embedding the conserve library, perhaps in a GUI or a web
   service, that want to route messages, progress indications, and other
   interactions through their own code. These applications conceivably have
   multiple Conserve operations happening on different threads simultaneously,
   and want to keep the output separate. (Although, this is in tension with Rust
   logging's inherently-global concept.)

4. When run from unit tests or library API tests, a singleton UI won't work,
   because multiple tests run concurrently in different threads, and Rust
   captures stdout from only the thread on which the test starts, and even then
   in a limited way. (Tests that run the `conserve` binary as a subprocess have
   more freedom, including running it on a pseudoterminal.)

Log messages should be written through Rust's (fairly) standard `log` crate.
Listeners for logs can be configured only globally and only once. There should
be an option to write logs to a file, as well as to the terminal, and at a
different level of detail.

This implies:

* The library will emit logs but will not by default configure any log targets,
  so that applications can choose the target they want.

* The terminal UI, when active, must provide a log target, so that log messages
  can be interleaved with progress messages.

* Since the terminal UI is a log target, it must be constructed just once near
  program startup, and therefore it cannot be on during in-process tests.

Rather than directly constructing progress bars, core library code should send
messages to observer objects, passed in by the application. These can then be
configured to either draw progress bars to the terminal, or do nothing, or do
something else.

Progess bars are drawn only for the small number of main loops that are expected
to take a long time, and don't implicitly pop up due to IO.
