# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Write a band of files to the archive."""

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


def write_band(file_names, to_filename):
    # TODO(mbp): Don't overwrite existing files.
    data_file = open(to_filename + '.d', 'wb')
    index_file = open(to_filename + '.i', 'wb')
    data_sha = sha.sha()

    block_index = dura_pb2.BlockIndex()
    for file_name in file_names:
        st = os.lstat(file_name)

        _log.info('store %s' % file_name)

        if stat.S_ISREG(st.st_mode):
            ptype = dura_pb2.FileIndex.REGULAR
            # TODO(mbp): stream content for large files
            file_content = open(file_name).read()
        elif stat.S_ISDIR(st.st_mode):
            ptype = dura_pb2.FileIndex.DIRECTORY
            file_content = None
        elif stat.S_ISLNK(st.st_mode):
            ptype = dura_pb2.FileIndex.SYMLINK
            # TODO(mbp): Race here between discovering it's a link,
            # and trying to read it.
            file_content = os.readlink(file_name)
        else:
            # TODO(mbp): Maybe eventually store them too
            _log.warning("skipping special file %r, %r",
                file_name, stat)
            continue

        file_index = block_index.file.add()
        file_index.file_type = ptype
        file_index.path = file_name
        if file_content is not None:
            file_index.data_length = body_length = len(file_content)
            if body_length:
                file_index.data_sha1 = sha.sha(file_content).digest()
                file_index.data_offset = data_file.tell()

            data_file.write(file_content)
            data_sha.update(file_content)

    block_index.data_sha1 = data_sha.digest()
    block_index.data_length = data_file.tell()
    index_file.write(block_index.SerializeToString())

    data_file.close()
    index_file.close()

    # TODO(mbp): Maybe also store the as-compressed md5 so that we can check it
    # against a hash provided by the storage system, without reading back the
    # whole thing?


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
