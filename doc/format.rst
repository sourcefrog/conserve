***********
Dura format
***********

Concepts
********

The backup *source* is a local directory.  (Actually, a local directory
subject to some exclusion filters.)

A backup *archive* is a possibly-remote directory, containing archive
files.

Within an archive there are multiple *tiers* for
incremental/hierarchical backups.  (For example, for monthly, weekly,
daily backups.)

Within each tier, there are multiple *bands*.  (For example, "the monthly
backup made on 2012-10-01.")  A band may have a *parent* band, which is
one particular band in the immediately lower tier.  

A band contains file contents and metadata.

A band is composed of a *header*, *footer*, and multiple *blocks*, each
with a *block footer* and a *block tarball*.

The block tarball is, a plain unix tarball of the files included in that
block.  Each file is stored entirely within one block.

The block footer contains 

 - a list of all files included in the block, and for each:
   - the utf-8 name of the file
   - the hash of that file as it was stored in the tarball.  
   - the mtime of the file
   - possibly ownership, permissions, and other filesystem metadata
 - the hash of the overall tarball

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
************

Protobufs for all metadata: handles evolution, somewhat selfdescribing 
but also very efficient.

Format file
***********

In the root directory of the archive there is a file called `format`, 
which is a protobuf with::

    magic: "dura archive"
    read_version: 0
    write_version: 0

Clients must have a major version at least equal to those values to correctly
read and write the archive.
