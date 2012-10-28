# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""A band of files, representing a pass over the source tree."""

import logging
import os
import socket
import stat
import sys
import time

from duralib.ioutils import (
    write_proto_to_file,
    )
from duralib.proto import dura_pb2


_log = logging.getLogger('dura')


class Band(object):
    """A Band stores versions from one pass over the source tree.

    The band contains blocks, each of which has actual content from
    a number of files.
    """

    # Prefix on band directory names.
    name_prefix = 'b'

    head_name = 'BAND-HEAD'
    tail_name = 'BAND-TAIL'

    def __init__(self, archive, band_number):
        self.archive = archive
        self.open = False  # can only be true on writers
        self.band_number = _canonicalize_band_number(band_number)
        self.path = os.path.join(
            self.archive.path,
            self.name_prefix + self.band_number)

    def __repr__(self):
        return '%s(path=%r)' % (
            self.__class__.__name__,
            getattr(self, 'path'))

    def relpath(self, subpath):
        """Convert band-relative path to an absolute path."""
        return os.path.join(self.path, subpath)

    @classmethod
    def match_band_name(cls, filename):
        """Try to interpret a filename as a band name.

        Returns:
            A band number, if the filename is a band, otherwise None.
        """
        if filename.startswith(cls.name_prefix):
            return filename[len(cls.name_prefix):]

    def read_head(self):
        self.head_pb = read_proto_from_file(BandHead, self.relpath(head_name))


class BandReader(Band):
    """A band open for readonly access.

    May or may not be complete.  May be concurrently written by other processes.
    """


class BandWriter(Band):
    """Writes in to a band.

    Attributes:
        open (bool): True if the band is still open to add files.
    """

    def start_band(self):
        _log.info("start band %r" % self)
        assert not self.open
        self.open = True
        os.mkdir(self.path)
        head_pb = dura_pb2.BandHead()
        head_pb.band_number = self.band_number
        head_pb.start_unixtime = int(time.time())
        head_pb.source_hostname = socket.gethostname()
        write_proto_to_file(head_pb, self.relpath(self.head_name))
        self.head_pb = head_pb

    def finish_band(self):
        """Write the band tail; after this no changes are allowed."""
        _log.info("mark band %r finished" % self)
        assert self.open
        self.open = False
        tail_pb = dura_pb2.BandTail()
        tail_pb.band_number = self.band_number
        # TODO(mbp): set block count!
        tail_pb.end_unixtime = int(time.time())
        write_proto_to_file(tail_pb, self.relpath(self.tail_name))


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
