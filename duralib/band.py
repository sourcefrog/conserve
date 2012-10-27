# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""A band of files, representing a pass over the source tree."""

import logging
import os
import sha
import stat
import sys

from duralib.proto import dura_pb2


_log = logging.getLogger('dura')


class Band(object):
    """A Band stores versions from one pass over the source tree.

    The band contains blocks, each of which has actual content from
    a number of files.
    """

    # Prefix on band directory names.
    name_prefix = 'b'

    def __init__(self, archive, band_number):
        self.archive = archive
        self.band_number = _canonicalize_band_number(band_number)
        self.path = os.path.join(
            self.archive.path,
            self.name_prefix + self.band_number)

    def relpath(self, subpath):
        """Convert band-relative path to an absolute path."""
        return os.path.join(self.path, subpath)

    def create_directory(self):
        _log.info("create band directory %s" % self.path)
        os.mkdir(self.path)

    @classmethod
    def match_band_name(cls, filename):
        """Try to interpret a filename as a band name.

        Returns:
            A band number, if the filename is a band, otherwise None.
        """
        if filename.startswith(cls.name_prefix):
            return filename[len(cls.name_prefix):]


def read_index(index_file_name):
    with open(index_file_name, 'rb') as index_file:
        block_index = dura_pb2.BlockIndex()
        block_index.ParseFromString(index_file.read())
        return block_index

def _canonicalize_band_number(band_number):
    return '%04d' % int(band_number)


def cmp_band_numbers(n1, n2):
    """Compare band number strings, treating them as sequences of integers.

    Args:
        n1, n2: strings, like "0000", "0001-1234".

    Returns:
        -1 if n1<n2, +1 if n1>n2, 0 if the same.
    """
    n1l = [int(x) for x in n1.split('-')]
    n2l = [int(x) for x in n2.split('-')]
    return cmp(n1l, n2l)
