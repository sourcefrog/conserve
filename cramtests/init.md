To create an archive:

    $ conserve init arch
    Created new archive in "arch"

This makes a new directory that contains just one file, the `CONSERVE-ARCHIVE`
header file:

    $ ls -a arch
    .
    ..
    CONSERVE

The header is readable json containing only a version number:

    $ cat arch/CONSERVE
    {"conserve_archive_version":"0.2.0"}
