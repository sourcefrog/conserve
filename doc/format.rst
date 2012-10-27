Dura format
===========

The backup *source* is a local directory.  (Actually, a local directory
subject to some exclusion filters.)

Archive
-------

A backup *archive* is a possibly-remote directory, containing archive
files.

Within an archive there are multiple *tiers* for
incremental/hierarchical backups.  (For example, for monthly, weekly,
daily backups.)

Bands
-----

Within each tier, there are multiple *bands*.  (For example, "the monthly
backup made on 2012-10-01.")  A band may have a *parent* band, which is one
particular band in the immediately lower tier.

Bands are numbered hierarchically across tiers and sequentially within
a tier, starting at 0.  So the first base tier band in the whole archive
is 0000, the first incremental band on top of it is 0000-0000,
and so on.  The numbers are zero-padded to four digits in each
area, so that they will be grouped conveniently for humans looking at
naively sorted listings of the directory.  (Dura does not rely on them
being less than five digits, or on the transport returning any particular
ordering.)

Bands are represented as a subdirectory within the archive directory,
as `b` followed by the number.  All bands are directly in the
archive directory, not nested by tier.  (This is done so that their
directories have self-contained names, and the paths don't get too
long.)  For example::

    my-archive/
      b0000
      b0000-0000
      b0000-0001
      b0000-0001-0000

A band contains file contents and metadata.

A band is composed of a *header*, *footer*, and multiple *blocks*, each
with a *block index* and a *block data*.

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

The combination of a band, its parent band (if any), parent's parents, etc
is a *version*: extracting all the contents of the version recreates
the source directory as it existed at the time the most recent band
was recorded.  (Modulo any changes that happened during the recording
of that band.)

Generalities
============

Protobufs for all metadata: handles evolution, somewhat selfdescribing
but also very efficient.

Format file
-----------

In the root directory of the archive there is a file called `DURA-ARCHIVE`,
which is an `ArchiveHeader` protobuf with::

    magic: "dura archive"
