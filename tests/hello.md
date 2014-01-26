Everything in Conserve is done through a subcommand to the `conserve` command:

    $ conserve
    Usage:
      conserve [-v] init <dir>
    [1]

All log output has a prefix which includes the message severity (E=error,
I=info, etc); the MMDD date; the time; the PID; and the source location.

To create an archive:

    $ conserve init a

This makes a new directory that contains just one file, the `CONSERVE-ARCHIVE`
header file:

    $ ls a -a -1
    .
    ..
    CONSERVE
