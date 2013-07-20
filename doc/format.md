Conserve format
===============

Generalities
------------

All header data is stored as [Google Protobufs][1].

Example in this document are expresed as (approximately) the text format of protobufs, but the on-disk format is the compact binary form.

Source
------

The backup *source* is a local directory.  (Actually, a local directory
subject to some exclusion filters.)

Archive
-------

A backup *archive* is a possibly-remote directory, containing archive
files.

In the root directory of the archive there is a file called `CONSERVE-ARCHIVE`,
which is an `ArchiveHeader` protobuf containing:

    magic: "conserve backup archive"

Tiers
-----

Within an archive there are multiple *tiers* for incremental/hierarchical
backups.  (For example, for monthly, weekly, daily backups.)  Tiers are not
directly represented on disk; they're just a logical grouping.

Bands
-----

Within each tier, there are multiple *bands*.  (For example, "the monthly
backup made on 2012-10-01.")  A band may have a *parent* band, which is one
particular band in the immediately lower tier.

A band may be *open*, while it is receiving data, or *finished* when
everything from the source has been written.  Bands may remain open
indefinitely, across multiple Conserve invocations, until they are finished.
Once the band is finished, it will not be changed.

Bands are numbered hierarchically across tiers and sequentially within
a tier, starting at 0.  So the first base tier band in the whole archive
is 0000, the first incremental band on top of it is 0000-0000,
and so on.  The numbers are zero-padded to four digits in each
area, so that they will be grouped conveniently for humans looking at
naively sorted listings of the directory.  (Conserve does not rely on them
being less than five digits, or on the transport returning any particular
ordering; bands numbered over 9999 are supported.)

Bands are represented as a subdirectory within the archive directory,
as `b` followed by the number.  All bands are directly in the
archive directory, not nested by tier.  (This is done so that their
directories have self-contained names, and the paths don't get too
long.)  For example:

    my-archive/
      b0000/
      b0000-0000/
      b0000-0001/
      b0000-0001-0000/

A band contains file contents and metadata.

A band is composed of a *head*, *tail*, and multiple *blocks*, each
with a *block index* and a *block data*.

A band head is a file `BAND-HEAD` containing a `BandHead` protobuf.

A band tail is a file `BAND-TAIL` containing a `BandTail` protobuf, only for
finished bands: it is the presence of this file that defines the band as
complete.


Blocks
------

A block contains the complete text of one or more files.  The files are
stored in sorted order.  The block data is simply the concatenation of
all the file texts.

The block index is a protobuf describing the files within the block:

 - a list of all files included in the block, and for each:
   - the name of the file
   - the hash of that file as it was stored in the tarball.
   - the mtime of the file
   - possibly ownership, permissions, and other filesystem metadata
 - the length and hash of the overall block data file

Band header contains:

 - the time the band started being recorded

Band footer contains:

 - the time the band started and ended
 - the hash of all of the block footers

Blocks are stored within their band directory, and numbered starting at 0 within
each band.

For each block there is an index file and and a data file. The index file
starts with `a` and the data file starts with `d`, followed by the decimal
block number padded to six digits.

The contents of both the index and data files are gzip-compressed.

So, for example:

    my-archive/
      b0000/
        a000000
        d000000


Versions
--------

The combination of a band, its parent band (if any), parent's parents, etc
is a *version*: extracting all the contents of the version recreates
the source directory as it existed at the time the most recent band
was recorded.  (Modulo any changes that happened during the recording
of that band.)

[1]: [https://code.google.com/p/protobuf/]