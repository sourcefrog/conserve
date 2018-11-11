# Conserve format

## Generalities

All metadata is stored as json dictionaries.

## Software version

Conserve archives include the version of the software that wrote them, which is
an _x.y.z_ tuple.  See [versioning.md](versioning.md) for the semantics.

## Filenames

Files have names in the source and restore directories, and within the archive.

In source and restore directories, file naming is defined by the OS: on Windows
as UTF-16, on OS X as UTF-8 and on Linux as an arbitrary 8-bit encoding.

(Linux filenames are very commonly UTF-8, but there are important exceptions:
users who choose to use different encodings for whole filesystems; network or
USB filesystems using different encodings; files is source trees that are
intentionally in odd encodings; and files that accidentally have anomalous
names.  It is useful to include the occasionally oddly-named file in the
backup, and also for users with non-UTF-8 encodings to be able to configure
this. The filename encoding is not easily detectable.  Linux does require that
the separator `/` have the same byte value.)

In the archive, Conserve uses "apaths" as a platform-independent path format.

apaths are stored as UTF-8 byte strings. UTF-8 filenames are
stored as received from the OS with no normalization.

apaths always have `/` separators and start with a `/`.

None of the apath components can be `.`, `..`, or empty.

The apath `/` within the archive is the top source directory.

Filenames are treated as case-sensitive.

## Filename sorting

There is a total order between filenames.  Within a band, entries are
stored in this order.

This ordering allows binary search for files within the index, and to
efficiently list the direct or recursive contents of a directory.

The ordering puts all the direct contents of a directory together, followed
by those of each of its children.

The order is defined as: split the filenames into a directory part
and a non-empty tail part.  Compare by the directory first using a
byte-by-byte comparison of their (typically UTF-8) byte string form.
Then, similarly compare the filenames.

Note that this is not the same as a simple comparison of the strings.

Rationale: This ordering makes several important read operations on the index
efficient.  Since the index overall is ordered, we can binary search into it.
Since all the direct children of a directory are grouped together, a
non-recursive listing of a single directory (e.g. from a web ui or FUSE) is a
single contiguous read.  And, a recursive listing of a single directory (e.g.,
to restore that directory) is also a single contiguous read of the directories
and then all their subtrees.

## Archive

A backup *archive* is a directory, containing archive files.

Archives can be stored on cloud or other remote storage.
The archive makes minimal assumptions about the filesystem it's stored on: in
particular, it need not support case sensitivity, it need not store times or
other metadata, and it only needs to support 8.3 characters.  It must supported
nested subdirectories with a total path length up to 100 characters.

Archive filesystems must allow many files per directory.

## Archive header

In the root directory of the archive there is a file called `CONSERVE`,
which is contains a json dict (with no compression):

    {"conserve_archive_version":"0.5"}

(For pre-1.0 versions of Conserve, older formats are described in the version
of this file from the relevant release source tree.)

## Bands

Within an archive, there are multiple *bands*, identified by a name starting
with `b` and followed by a sequence of integers.

A band that is not in the base tier has a *parent band* in the immediately
lower tier.

A band can be *complete*, while it is receiving data, or *incomplete* when
everything from the source has been written.  Bands may remain incomplete
indefinitely, across multiple Conserve invocations, until they are finished.
Once the band is completed, it will not be changed.

Bands can be *top level* in which case their index contains a list of all
entries (files, directories, and symlinks) in that tree. Or, they can be
*child bands*, in which case they contain only changes relative to a parent
band's index. (Child bands are not implemented as of Conserve 0.5.)

All band names start with the character `b`. Top level bands are numbered
sequentially from `b0000`. Child bands have additional numbers appended to
their parent's name, like `b0000-0000`. 

The numbers are zero-padded to four digits in each area, so that they will be
grouped conveniently for humans looking at naively sorted listings of the
directory.  (Conserve does not rely on them being less than five digits, or on
the transport returning any particular ordering; bands numbered over 9999 are
supported.)

Bands are represented as a subdirectory within the archive directory, as `b`
followed by the number.  All bands are in the top-level archive directory.

    my-archive/
      b0000/
      b0000-0000/
      b0000-0001/
      b0000-0001-0000/

## Band head

A band head is a file `BANDHEAD` containing a json dictionary.

The head file is written when the band is first opened and then it is
not changed again.

The head file contains:

 - `start_time`: the Unix time the band was started

## Band tail

A band tail is a file `BANDTAIL` containing a json dictionary, only for
finished bands: it is the presence of this file that defines the band as
closed.

Band footer contains:

 - `end_time`: the Unix time the band started and ended


## Data blocks

Data blocks contain parts of the contents of stored files.

One data block may contain data for a whole file, the concatenated text for
several files, or part of a file.  The index entries describe which parts of
which blocks are concatenated to recreate the file.

The writer can choose the data block size, except that both the uncompressed
and compressed blocks must be <1GB, so they can reasonably fit in memory.
The writer might choose to break the file not at a fixed size but instead at
some boundary it thinks will be stable as the file changes, for example using
an rsync-like rolling checksum.

The name of the data block file is the BLAKE2 hash of the uncompressed
contents.

The blocks are spread across a single layer of subdirectories, where each
subdirectory is the first three hex characters of the name of the contained
block files.

Data block are compressed in the Snappy format
<https://github.com/google/snappy>.

## Blockdir

Starting from format 0.5, there is a single blockdir per archive, containing
all content blocks from all bands. This is the `d/` directory directly with in
the archive directory.

Any band in the archive can refer to blocks from this single blockdir.

Blocks can be garbage collected (only) by reading the list of all present
blocks, and then subtracting the ones referenced by any band's index.  Anything
left unreferenced can be deleted. It's important the operation be done in this
order, so that it's safe in the case a band is being written concurrently with
the gc operation, or if the filesystem is not quite coherent.

## Index hunks

Index hunks contain the name and metadata of a stored file, plus a
reference to the data hunks holding its full text.

Index hunks are named with decimal sequence numbers padded to 9 digits.

Index hunks are stored in an `i/` subdirectory of the band, and then
in a subdirectory for the sequence number divided by 10000 and
padded to five digits.  So, the first block is `i/00000/000000000`.

Index hunks are stored in json and also Snappy compressed.

Stored files are in order by filename across all of the index hunks
within a band.

The number of files described within a single index hunk file is
arbitrary and may be chosen to control the number of outstanding data
blocks or the length of the index hunk.

The uncompressed index hunk contains a json list each element of
which is a dict of

   - `apath`: the name of the file
   - `mtime`: in seconds past the unix epoch
   - ownership, permissions, and other filesystem metadata
   - `kind`: one of `"File"`, `"Dir"`, `"Symlink"`
   - `deleted`: true if it was present in a parent band and was
     deleted in this band
   - `addrs`: a list of tuples of:
     - `hash`: data block hash: from the current or any
       parent directory
     - `start`: the offset within the uncompressed content of the
       block for the start of this file
     - `length`: the number of bytes of uncompressed data block
       content to store in this file
     `target`: For symlinks, the string target of the symlink.

So, the length of any file is the sum of the `length` entries for all
its `addrs`.
