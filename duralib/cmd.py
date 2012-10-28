# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Abstracted commands.

This module contains command functions corresponding to cli
commands, but which (should) have no specific dependencies on either
the cli command line or the output format.  It is intended they
could be invoked by a different ui mechanism, showing their output
in a different way.
"""


import logging
import time


from duralib.archive import Archive
from duralib.timeutils import isotime


_log = logging.getLogger('dura')


def cmd_create_archive(args):
    """Make a new archive to hold backups.

    The destination directory will be created, and must not exist.
    """
    new_archive = Archive.create(args.archive_directory)
    _log.info("Created %s", new_archive)


def cmd_describe_archive(args):
    """Show summary information about an archive."""
    archive = Archive.open(args.archive_directory)
    _log.info("Opened archive %r", archive)


def cmd_backup(args):
    """Store a copy of source files in the archive.

    Creates a new archive version.

    Args:
      source_file [str]: Paths to store
      archive [str]: Path to existing archive
    """
    from duralib.backup import do_backup
    archive = Archive.open(args.archive)
    do_backup(args.source_file, archive)


def cmd_list_bands(args):
    """List bands in an archive.

    Incomplete bands are shown with a + next to their name.

    The start time and source hostname is also shown.
    """
    archive = Archive.open(args.archive)
    for band_name in archive.list_bands():
        if args.names_only:
            args.stdout.write("%s\n" % band_name)
            continue
        band = archive.open_band_reader(band_name)

        name_plus = band_name
        if not band.is_finished():
            name_plus += '+'

        print "%-8s" % name_plus,
        if band.head:
            print "  %s   %s" % (
                isotime(band.head.start_unixtime),
                band.head.source_hostname),
        print


def cmd_list_files(args):
    """List files in a backup band."""
    from duralib.dump import print_block_index
    archive = Archive.open(args.archive)
    band = archive.open_band_reader(args.band)
    block_index = band.read_block_index('000000')
    print_block_index(block_index, names_only=args.names_only)