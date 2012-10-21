# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line interface"""

import argparse
import logging


_log = logging.getLogger('dura')


def _make_parser():
    """Make an ArgumentParser"""
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(
        title='commands')

    parser_create_archive = subparsers.add_parser(
        'create-archive',
        help='Make a new archive to hold backups')
    parser_create_archive.set_defaults(cmd_func=cmd_create_archive)
    parser_create_archive.add_argument(
        'archive_directory',
        help='Local path to directory to be created')

    return parser


def cmd_create_archive(args):
    from duralib import archive
    new_archive = archive.Archive.create(args.archive_directory)
    _log.info("Created %s", new_archive)


def run(argv):
    parser = _make_parser()
    args = parser.parse_args()
    _log.debug("cli args: %r", args)
    args.cmd_func(args)
    return 0
