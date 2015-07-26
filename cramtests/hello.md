Everything in Conserve is done through a subcommand to the `conserve` command:

    $ conserve
    Invalid arguments.
    
    Usage:
        conserve init <dir>
        conserve --version
        conserve --help
    [1]

You can ask for help:

    $ conserve --help
    Conserve: an (incomplete) backup tool.
    Copyright 2015 Martin Pool, GNU GPL v2+.
    https://github.com/sourcefrog/conserve
    
    Usage:
        conserve init <dir>
        conserve --version
        conserve --help

`--version` shows the version with no fluff:

    $ conserve --version
    0.2.0

To create an archive:

    $ conserve init a
    Created archive in "a"

This makes a new directory that contains just one file, the `CONSERVE-ARCHIVE`
header file:

    $ ls -a a
    .
    ..
    CONSERVE
