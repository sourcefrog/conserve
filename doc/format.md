# Conserve format

## Generalities

All metadata is stored as json dictionaries.

## Software version

Conserve archives include the version of the software that wrote them, which is an
_x.y.z_ tuple. Changes to the `x` major version imply a non-backward-compatible change:
older versions may not be able to read it. Changes to the `y` minor version may
include backward-compatible extensions.  (See [Versioning](versioning.md).)


## Filenames

Files have names in the source and restore directories, and within the archive.
In source and restore directories, file naming is defined by the OS: on Windows as UTF-16,
on OS X as UTF-8 and on Linux as an arbitrary 8-bit encoding.  

(Linux filenames are very commonly UTF-8, but there are important exceptions: users who
choose to use different encodings for whole filesystems; network or USB filesystems
using different encodings; files is source trees that are intentionally in odd encodings; and
files that accidentally have anomalous names.  It is useful to include the occasionally
oddly-named file in the backup, and also for users with non-UTF-8 encodings to be able to
configure this. The filename encoding is not easily detectable.  Linux does require that the
separator `/` have the same byte value.)

In the archive, filenames are stored as UTF-8 byte strings. UTF-8 filenames are
stored as received from the OS with no normalization.

Filenames are always stored as Unix-style paths separated by `/` characters.
None of the path components can be `.`, `..`, or empty.

Filenames are treated as case-sensitive.

## Filename sorting

There is a total order between filenames.  Within a band, entries are
stored in this order.

The order is defined as: split the filenames into a directory part
and a non-empty tail part.  Compare by the directory first using a
byte-by-byte comparison of their (typically UTF-8) byte string form.
Then, similarly compare the filenames.

This means that all the files in a single directory are stored
together, and ahead of any subdirectories.

## Archive

A backup *archive* is a possibly-remote directory, containing archive
files.

The archive makes minimal assumptions about the filesystem it's stored on: in
particular, it need not support case sensitivity, it need not store times or
other metadata, and it only needs to support 8.3 characters.  It must supported
nested subdirectories but only of moderate length.

## Archive header

In the root directory of the archive there is a file called `CONSERVE`,
which is contains a json dict:

    {"conserve_archive_version":"0.2.0"}

## Tiers

Within an archive there are multiple *tiers* for incremental/hierarchical
backups.  (For example, for monthly, weekly, daily backups.)  Tiers are not
directly represented on disk; they're implicitly all the bands whose names
identify them as being in the same tier.

## Bands

Within each tier, there are multiple *bands*.  (For example, "the monthly
backup made on 2012-10-01.")  A band may have a *parent* band, which is one
particular band in the immediately lower tier.

A band may be *open*, while it is receiving data, or *closed* when
everything from the source has been written.  Bands may remain open
indefinitely, across multiple Conserve invocations, until they are finished.
Once the band is finished, it will not be changed.

Bands are numbered hierarchically across tiers and sequentially within
a tier, starting at 0.  So the first base tier band in the whole archive
is `0000`, the first incremental band on top of it is `0000-0000`,
and so on.  The numbers are zero-padded to four digits in each
area, so that they will be grouped conveniently for humans looking at
naively sorted listings of the directory.  (Conserve does not rely on them
being less than five digits, or on the transport returning any particular
ordering; bands numbered over 9999 are supported.)

Band directories contain a description of files that changed, or were deleted,
relative to their ancestor bands.  A copy of the source directory at a
particular time can be extracted by reading the closest band, plus all of its
parents.

Bands are represented as a subdirectory within the archive directory,
as `b` followed by the number.  Sub-bands are stored inside their
parent.

    my-archive/
      b0000/
      b0000/b0000/
      b0000/b0001/
      b0000/b0001/b0000/
      
## Band head

A band head is a file `BANDHEAD` containing a json dictionary.

The head file is written when the band is first opened and then it is
not changed again.

The head file contains:

 - the time the band was started


## Band tail

A band tail is a file `BANDTAIL` containing a json dictionary, only for
finished bands: it is the presence of this file that defines the band as
closed.

Band footer contains:

 - the time the band started and ended
 - the hash of all of the block footers


## Data hunks

Data hunks contain parts of the full text of stored files.

One data hunk may contain data for a whole file, the concatenated
text for several files, or just part of a file.  One data hunk
might be referenced from the index hunks of any number of files
from the current or any descendent band.

Data hunks may be of arbitrary compressed length, and arbitrary
uncompressed length, chosen at the writer's convenience.

All the data hunks for a band are stored within a `d/` subdirectory
of the band, and then within a directory for the first three characters
of their name.

Data hunks are gzip-compressed.

The name of the data hunk file is the BLAKE2 hash of the uncompressed
contents.


## Index hunks

Index hunks contain the name and metadata of a stored file, plus a
reference to the data hunks holding its full text.

Index hunks are stored in an `i/` subdirectory of the band, and
within that in a subdirectory named for the first three characters of
their name.

Index hunks are named with decimal sequence numbers padded to 9 digits.

Index hunks are stored in json and gzip compressed.

Stored files are in order by filename across all of the index hunks
within a band.

The number of files described within a single index hunk file is
arbitrary and may be chosen to control the number of outstanding data
blocks or the length of the index hunk.

The uncompressed index hunk contains a json list each element of
which is a dict of

   - `name`: the name of the file
   - `mtime`: in seconds past the unix epoch
   - possibly ownership, permissions, and other filesystem metadata
   - `type`: one of `"file"`, `"dir"`, `"symlink"`
   - `deleted`: true if it was present in a parent band and was
     deleted in this band
   - `blake2`: the BLAKE2 hash in hex of the full text of the file
   - `blocks`: a list of tuples of:
     - `block`: data block hash: from the current or any
       parent directory
     - `start`: the offset within the uncompressed content of the
       block for the start of this file
     - `length`: the number of bytes of uncompressed data block
       content to store in this file

So, the length of any file is the sum of the `length` entries for all
its `blocks`.
