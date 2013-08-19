To make a backup you must first have an archive:

    $ conserve init-archive a

Let's make some files to put into it

    $ mkdir src
    $ echo strawberry > src/f

And now back it up:

    $ conserve backup src/f a

This creates a new _band directory_ and some block data within it:

    $ ls a/b0000
    BANDHEAD
    BANDTAIL
    a000000
    d000000

TODO(mbp): Recursively backup directories.

Obviously you also want to be able to restore from it.  The restore command
takes an archive name, a destination directory, and optionally a list of files
to restore into that directory.  Existing files will be overwritten.

TODO: Actually test restoring from it.

TODO: List band contents.
