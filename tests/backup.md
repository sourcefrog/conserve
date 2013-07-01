To make a backup you must first have an archive:

    $ conserve init-archive a

Let's make some files to put into it

    $ mkdir src
    $ echo strawberry > src/f

And now back it up:

    $ conserve backup a src/f

This creates a new _band directory_ and some block data within it:

    $ ls a/b0000
    BAND-HEAD
    BAND-TAIL

TODO(mbp): Recursively backup directories.

TODO(mbp): Show the contents of the archive and restore from it.
