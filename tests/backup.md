To make a backup you must first have an archive:

    $ conserve init-archive myarchive

Make a file to put into it

    $ echo strawberry > afile

And now back it up:

    $ conserve backup afile myarchive

This creates a new _band directory_ and some block data within it:

    $ ls myarchive/b0000
    BANDHEAD
    BANDTAIL
    a000000
    d000000

TODO(mbp): Recursively backup directories.

Obviously you also want to be able to restore from it.  The restore command
takes an archive name, a destination directory, and optionally a list of
files to restore into that directory.  Existing files will be overwritten.

TODO: List band contents.

    $ conserve restore myarchive restoredir
    $ cat restoredir/afile
    strawberry

For safety, you cannot restore to the same directory twice:

    $ conserve -L restore myarchive restoredir
    error creating restore destination directory "restoredir": File exists
    [1]

TODO: Test that you can only backup into an already initialized archive.

There is a `validate` command that checks that an archive is internally
consistent and well formatted.  Validation doesn't compare the contents
of the archive to any external source.  Validation is intended to catch
bugs in Conserve, underlying software, or hardware errors -- in the
absence of such problems it should never fail.

Validate just exits silently and successfully unless problems are
detected.

    $ conserve validate myarchive
