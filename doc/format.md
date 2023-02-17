# Conserve format 0.6

## Overview

All the data written by Conserve is within an _archive_.

Schematically, data is contained as follows:

    archive directory, contains
        1 archive header file
        n bands, each containing
            1 band header file
            n index hunk files
            0..1 band tail file
        1 data block directory, containing
            n data block files

## Generalities

The archive makes minimal assumptions about the filesystem it's stored on: in
particular, it need not support case sensitivity, it need not store times or
other metadata, and it need only support ASCII filenames. It must supported
nested subdirectories with a total path length up to 100 characters.

Archive filesystems must allow many files per directory.

Conserve never changes files after they're written. Files are never deleted
aside from in an explicit data-deletion operation, none of which are implemented
as of Conserve 0.6.

On local filesystems, files are written through a write-and-rename, so should
appear atomically complete.

## Archive

A backup _archive_ is a directory, containing an _archive header_, a _data block
directory_, and any number of _band directories_.

### Archive header

In the root directory of the archive there is a file called `CONSERVE`, which is
contains a json dict, with no compression, with the following contents.

    {"conserve_archive_version": "0.6"}

For pre-1.0 versions of Conserve, increments in the minor version (the second
component) may imply a new archive format, and they are not guaranteed to
support older formats. That is to say, a build of Conserve from the 0.6 series
can be used to read an 0.6 archive.

See [versioning.md](versioning.md) for more on version compatibility.

## Apaths

Filenames in the archive are normalized to a format called an _apath_, which
defines a representation and an ordering.

Apaths are the same regardless of source OS, although some Apaths might be
impossible to write on certain filesystems. On filesystems that are
case-insensitive, or that do Unicode normalization, multiple Apaths might
identify the same file.

Apaths are stored as UTF-8 byte strings.

UTF-8 filenames are stored as received from the OS with no additional
normalization.

Apaths always have `/` separators.

Apaths always start with a `/`, which means the root of the source tree, which
is not (typically) the root of the filesystem.

None of the apath components can be `.`, `..`, or empty.

Filenames are treated as case-sensitive in Unicode.

There is a total order between apaths. In the index, files are stored in this
order. Trees are traversed in this order.

The order is defined as: split the filenames into a directory part and a
non-empty tail part. Compare by the directory first using a byte-by-byte
comparison of their UTF-8 form. If the directories differ, the that defines the
order of the paths. If the directories are the same, compare the filenames. Note
that this is not the same as a simple comparison of the strings.

### Rationale

Apath ordering puts all the direct contents of a directory together, followed by
those of each of its children. (A naive string comparison would be more like a
depth-first traversal, interleaving all the contents into a traversal of the
tree root directory.) An entire subtree is also contiguous, starting with its
top directory.

This ordering makes several important operations more efficient.

Since each directory is contiguous, it's easy to walk a source tree in apath
order. For each source directory, we read and sort all the contents, and can
then deal with them one after the other.

Since the index is ordered, we can binary search into it, for example to restore
or list a subdirectory. Whether doing a binary search or a simple linear scan,
we can tell immediately whether a given filename is present, or not.

Since all the direct children of a directory are grouped together, a
non-recursive listing of a single directory requires reading a single contiguous
subsequence of the index.

## Bands

Within an archive, there are multiple _bands_, each describing the contents of a
single version of the backup tree.

Bands are identified by a name starting with `b` and followed by a sequence of
one or more integers, separated by dashes.

Each band corresponds to a single version of the backup tree.

In Conserve 0.6, only bands with a single integer, called a _top level band_,
are generated or supported. Top level bands contain an index listing every entry
present in that tree. Top level bands are numbered sequentially from `b0000`.

A band can be _complete_, while it is receiving data, or _incomplete_ when
everything from the source has been written. Bands may remain incomplete
indefinitely, across multiple Conserve invocations, until they are finished.
Once the band is completed, it will not be changed. A band is complete if its
_band tail_ file exists, and incomplete otherwise.

Numbers in band indexes are zero-padded to four digits in each area, so that
they will be grouped conveniently for humans looking at naively sorted listings
of the directory. (Conserve does not rely on them being less than five digits,
or on the transport returning any particular ordering; bands numbered over 9999
are supported.)

Bands are represented as a subdirectory within the archive directory, as `b`
followed by the number. All bands are in the top-level archive directory.

    my-archive/
      b0000/
      b0000-0000/
      b0000-0001/
      b0000-0001-0000/

### Band head file

A band head is a file `BANDHEAD` containing an uncompressed json dictionary,
within the band directory.

The head file is written when the band is first opened and then it is not
changed again.

The head file contains:

- `start_time`: The Unix time, in seconds, when the band was started.
- `band_format_version`: The minimum program version to correctly read this
  band.
- `format_flags`: A list of strings indicating capabilities required to read
  this band correctly. If this is set and non-empty, then the `band_format_version`
  must be at least 23.2.0.

### Band tail file

A band tail is a file `BANDTAIL` containing a json dictionary, within the band
directory. It is the presence of this file that defines the band as complete.

Band footer contains:

- `end_time`: The Unix time, in seconds, that the band ended.
- `index_hunk_count`: The number of index hunks that should be present for this
  band. (Since 0.6.4.)

## Format flags

(None are defined yet.)

## Data block directory

An archive contains a single data block directory, which stores the compressed
body content of all files in the archive. This is the `d/` directory directly
with in the archive directory.

### Data blocks

Data blocks contain parts of the contents of stored files.

One data block may contain data for a whole file, the concatenated text for
several files, or part of a file.

The writer can choose the data block size, except that both the uncompressed and
compressed blocks must be <1GB, so they can reasonably fit in memory.

The name of the data block file is the BLAKE2 hash of the uncompressed contents.

The blocks are spread across a single layer of subdirectories, where each
subdirectory is the first three hex characters of the name of the contained
block files.

Data block are compressed in the Snappy format
<https://github.com/google/snappy>: the 'raw' format without framing.

## Index

Conceptually, the index stores a list of _index entries_ in apath order.
Externally, the index is broken into several numbered _index hunk_ files, each
containing many index entries.

### Index entries

_Index entries_ contain the name and metadata of a stored file, plus a reference
to the data hunks holding its full text.

An index entry is a json dict with keys

- `apath`: the apath of the file
- `mtime`: integer seconds past the Unix epoch
- `mtime_nanos`: (optional) fractional part of the mtime, as nanoseconds.
- `kind`: one of `"File"`, `"Dir"`, `"Symlink"`
- `unix_mode`: the unix mode bits consisting of the sticky bit, set uid bit, 
    set gid bit, and permission bits
- `user`: optionally, a string specifying the file owner
- `group`: optionally, a string specifying the primary group owner
- `addrs`: a list of tuples of:
  - `hash`: data block hash: from the current or any parent directory
  - `start`: the offset within the uncompressed content of the block for the
    start of this file
  - `length`: the number of bytes of uncompressed data block content to store in
    this file
- `target`: For symlinks, the string target of the symlink.

So, the length of any file is the sum of the `length` entries for all its
`addrs`.

### Index hunks

Index hunks are named with decimal sequence numbers padded to 9 digits, starting
at 0.

Index hunks are stored in an `i/` subdirectory of the band, and then in a
subdirectory for the sequence number divided by 10000 and padded to five digits.
So, the first block is `i/00000/000000000`.

Index hunks are serialized as json and then Snappy compressed.

An index hunk is a json list of index entries.

Entries are sorted by apath both within each hunk, and across all hunks.

The number of files described within a single index hunk file is arbitrary and
may be chosen to control the number of outstanding data blocks or the length of
the index hunk.

## Garbage collection lock

New in 0.6.7: A `GC_LOCK` file in the archive directory indicates that a
garbage collection operation is underway, and new backups or gc operations
cannot start. The file contains an empty json dict, `{}`. More keys may be
added in future.
