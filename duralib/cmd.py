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


def cmd_dump_index(args):
    """Show debug information about index files."""
    from duralib.dump import dump_index_block
    for index_file_name in args.index_files:
        dump_index_block(index_file_name)


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
    """List bands in an archive."""
    archive = Archive.open(args.archive)
    for band_name in archive.list_bands():
        if args.names_only:
            args.stdout.write("%s\n" % band_name)
            continue
        band = archive.open_band_reader(band_name)
        print "%s" % band_name,
        if band.head:
            print "  %s   %s" % (
                time.ctime(band.head.start_unixtime),
                band.head.source_hostname),
        print

