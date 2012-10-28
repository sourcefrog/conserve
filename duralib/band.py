# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""A band of files, representing a pass over the source tree."""

import os
import socket
import time

from duralib import _log

from duralib.block import (
    canonical_block_number,
    match_block_index_name,
    BlockWriter,
    )
from duralib.ioutils import (
    read_proto_from_file,
    write_proto_to_file,
    )
from duralib.proto.dura_pb2 import (
    BandHead,
    BandTail,
    BlockIndex,
    )


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
        self.path = self.archive.relpath(
            self.name_prefix + self.band_number)

    def __repr__(self):
        return '%s(path=%r)' % (
            self.__class__.__name__,
            getattr(self, 'path'))

    def relpath(self, subpath):
        """Convert band-relative path to an absolute path."""
        return os.path.join(self.path, subpath)

    def index_file_path(self, block_number):
        return self.relpath('d' + canonical_block_number(block_number) + '.i')

    @classmethod
    def match_band_name(cls, filename):
        """Try to interpret a filename as a band name.

        Returns:
            A band number, if the filename is a band, otherwise None.
        """
        if filename.startswith(cls.name_prefix):
            return filename[len(cls.name_prefix):]

    def read_head(self):
        """Read head if possible."""
        self.head = read_proto_from_file(
            BandHead, self.relpath(self.head_name))

    def read_block_index(self, block_number):
        return read_proto_from_file(
            BlockIndex, self.index_file_path(block_number))

    def read_tail(self):
        """Read tail if present and possible."""
        self.tail = read_proto_from_file(
            BandTail, self.relpath(self.tail_name))
        return self.tail

    def list_blocks(self):
        """Return a sorted list of blocks in this band."""
        results = []
        for filename in os.listdir(self.path):
            number = match_block_index_name(filename)
            if number is not None: results.append(number)
        results.sort(cmp=cmp_band_numbers)
        return results

    def next_block_number(self):
        # TODO(mbp): Unify with allocation in bands?
        existing_blocks = self.list_blocks()
        if not existing_blocks:
            next_number = 0
        else:
            next_number = int(existing_blocks[-1]) + 1
        return '%06d' % next_number

    def create_block(self):
        # TODO(mbp): Could cache the number, which might be faster for slow transports.
        number = self.next_block_number()
        writer = BlockWriter(self, number)
        writer.begin()
        return writer


class BandReader(Band):
    """A band open for readonly access.

    May or may not be complete.  May be concurrently written by other processes.
    """

    def is_finished(self):
        return os.path.isfile(self.relpath(self.tail_name))


class BandWriter(Band):
    """Writes in to a band.

    Attributes:
        open (bool): True if the band is still open to add files.
    """

    def start_band(self):
        _log.info("begin %r", self)
        assert not self.open
        self.open = True
        os.mkdir(self.path)
        head_pb = BandHead()
        head_pb.band_number = self.band_number
        head_pb.start_unixtime = int(time.time())
        head_pb.source_hostname = socket.gethostname()
        write_proto_to_file(head_pb, self.relpath(self.head_name))
        self.head = head_pb

    def finish_band(self):
        """Write the band tail; after this no changes are allowed."""
        _log.info("finish %r", self)
        assert self.open
        self.open = False
        tail_pb = BandTail()
        tail_pb.band_number = self.band_number
        tail_pb.block_count = int(self.next_block_number())
        tail_pb.end_unixtime = int(time.time())
        write_proto_to_file(tail_pb, self.relpath(self.tail_name))

    def is_finished(self):
        return not self.open


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
