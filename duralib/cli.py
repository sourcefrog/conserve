# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line interface"""

import argparse
import logging


import duralib
from duralib.archive import Archive
from duralib import cmd


_log = logging.getLogger('dura')


def _first_line(s):
    """Return the first line of s, without the newline."""
    return s.split('\n', 1)[0]


def _parser_for_cmd(cmd_name):
    cmd_func = cmd.__dict__['cmd_' + cmd_name.replace('-', '_')]
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
    parser.add_argument('--version', action='version',
        version='dura ' + duralib.__version__)

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

    cp = _parser_for_cmd('backup')
    cp.add_argument(
        'source_file', nargs=argparse.ONE_OR_MORE,
        help='File to store')
    cp.add_argument(
        'archive',
        help='Existing archive directory')

    cp = _parser_for_cmd('list-bands')
    cp.add_argument('archive', help='Path of archive directory')
    cp.add_argument(
        '--names-only', '-q',
        help='Just list band names.',
        action='store_true')

    cp = _parser_for_cmd('list-files')
    cp.add_argument('archive', help='Path of archive directory')
    cp.add_argument('band', help='Number of band')
    cp.add_argument(
        '--names-only', '-q',
        help='Just list file names.',
        action='store_true')

    return parser


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


def run_command(argv, stdout=None):
    args = parse_command(argv)
    # TODO(mbp): More elegant way to pass it?
    args.stdout = stdout
    args.cmd_func(args)
    return 0
