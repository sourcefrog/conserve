Conserve user guide
===============

Conserve is a backup system: from local files, to files on other local filesystems.


Creating an archive
-------------------

Backed-up files are stored in an *archive* directory.  There should be one
archive per source directory to be backed up.

To start making a backup:

    % conserve create-archive /backup/my-src.conserve

This creates a directory containing a header file.


Making a backup
---------------

To store a backup of some source files into the archive, use the
`backup` command.  In the current version all files must be explicitly
listed, eg with the `find` command:

    % conserve backup `find ~/src` /backup/my-src.conserve


Version
-----

Each backup command creates a new backup *band*, which contains one
version of all files from the source.

Bands are only ever appended to the archive, and they're identified by
a string of integers starting with b, like `b0000`.


Examining bands
---------------

The `list-versions` command shows all the bands in an archive:

    % conserve list-versions /backup/my-src.conserve
    0000       2012-12-02T16:24:33   conservetesthost.local
    0001       2012-12-02T16:24:45   conservetesthost.local

`list-files` shows all the files in a band, including the
time they were made and the host from which they were made.
Like all commands that read a band from an archive, it operates
on the most recent by default.

Validation
----------

`conserve validate` checks whether the contents of an archive are internally
consistent.  It makes no reference to a source directory, just checks that
the archive seems to represent reasonable data and that it can all be
read and interpreted.
