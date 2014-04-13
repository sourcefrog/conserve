Everything in Conserve is done through a subcommand to the `conserve` command:

    $ conserve
    conserve - a robust backup program
    
    Copyright 2012-2013 Martin Pool
    Licenced under the GNU General Public Licence, version 2 or later.
    Conserve comes with ABSOLUTELY NO WARRANTY of any kind.
    
    Usage:
      conserve init DIR
      conserve backup ARCHIVE FILE...
    
    Options:
      --help        Show help.
      --version     Show version.
      -v            Be more verbose.

To create an archive:

    $ conserve init a

This makes a new directory that contains just one file, the `CONSERVE-ARCHIVE`
header file:

    $ ls a -a -1
    .
    ..
    CONSERVE
