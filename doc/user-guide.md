Dura user guide
===============

Dura is a backup system: from local files, to files on other local filesystems.


Creating an archive
-------------------

Backed-up files are stored in an *archive* directory.  There should be one 
archive per source directory to be backed up.

To start making a backup:

    dura create-archive /backup/my-src.dura

This creates a directory containing a header file.


Making a backup
---------------

To store a backup of some source files into the archive, use the 
`backup` command.  In the current version all files must be explicitly
listed, eg with the `find` command:

    dura backup `find ~/src` /backup/my-src.dura


Bands
-----

Each backup command creates a new backup *band*, which contains one 
version of all files from the source.  

Bands are only ever appended to the archive, and they're identified by 
sequential integers.


Examining bands
---------------

The `list-bands` command shows all the bands in an archive.

`list-files` shows all the files in a particular band, including the 
time they were made and the host from which they were made.


Validation
----------

`dura validate` checks whether the contents of an archive are internally
consistent.  It makes no reference to a source directory, just checks that
the archive seems to represent reasonable data and that it can all be 
read and interpreted.