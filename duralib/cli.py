# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line interface"""

import argparse
import logging


from duralib.archive import Archive


_log = logging.getLogger('dura')


def _first_line(s):
    """Return the first line of s, without the newline."""
    return s.split('\n', 1)[0]


def _parser_for_cmd(cmd_name):
    cmd_func = globals()['cmd_' + cmd_name.replace('-', '_')]
    cp = _cmd_subparsers.add_parser(
        cmd_name,
        help=_first_line(cmd_func.__doc__),
        description=cmd_func.__doc__,
        )
    cp.set_defaults(cmd_func=cmd_func)
    return cp


def _make_parser():
    """Make an ArgumentParser"""
    parser = argparse.ArgumentParser()
    global _cmd_subparsers
    _cmd_subparsers = parser.add_subparsers(
        title='commands',
        metavar='command',  # argparse arg metavars are in lowercase.
        )

    cp = _parser_for_cmd('create-archive')
    cp.add_argument(
        'archive_directory',
        help='Local path to directory to be created')

    cp = _parser_for_cmd('describe-archive')
    cp.add_argument(
        'archive_directory',
        help='Local path to archive directory')

    cp = _parser_for_cmd('dump-index')
    cp.add_argument(
        'index_files',
        nargs='*',
        help='Path to a .i index file to dump')

    cp = _parser_for_cmd('backup')
    cp.add_argument(
        'source_file', nargs=argparse.ONE_OR_MORE,
        help='File to store')
    cp.add_argument(
        'archive',
        help='Existing archive directory')

    return parser


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
    """
    # Not implemented



# Lazily initialized ArgumentParser.
_parser = None

def parse_command(argv):
    """Parse a command line; return parsed args.

    The returned args contain a cmd_func that can be called, passing the
    args, to actually run the command.
    """
    global _parser
    if _parser is None:
        _parser = _make_parser()
    args = _parser.parse_args(argv)
    _log.debug("cli args: %r", args)
    return args


def run_command(argv):
    args = parse_command(argv)
    args.cmd_func(args)
    return 0
