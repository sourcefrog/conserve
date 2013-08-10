Everything in Conserve is done through a subcommand to the `conserve` command:

    $ conserve
    E*] No command given (glob)
    [1]

Most of the tests give it the -L option to suppress the log prefix and avoid
unwanted variation.

    $ conserve -L
    No command given
    [1]

All log output has a prefix which includes the message severity (E=error,
I=info, etc); the MMDD date; the time; the PID; and the source location.

To create an archive:

    $ conserve init-archive a

This makes a new directory that contains just one file, the `CONSERVE-ARCHIVE`
header file:

    $ ls a -a -1
    .
    ..
    CONSERVE
