# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").
#
# vim: sw=4 et

"""Validate an archive, or components of it.

Validation means checking that the archive is well-formed and internally
consistent: for example, that the hashes in block indexes correspond to the
data actually stored in the block data files.  There's no guarantee that 
the data matches any actual source but it does attempt to show it matches
one possible source.
"""

# TODO(mbp): Allow validating just a single block, or a single band, etc.

import logging
import os
import sha
import stat
import sys

from google.protobuf import text_format

from duralib import _log
from duralib.archive import Archive
from duralib.proto import dura_pb2


def validate_archive(archive_path):
    """Validate an archive and all its contents."""
    _log.info("start validating %s", archive_path)
    archive = Archive.open(archive_path)
    _log.info("validation of %s succeeded", archive_path)




def check_block(band_filename):
    data_file = open(band_filename + '.d', 'rb')
    index_file = open(band_filename + '.i', 'rb')
    data_sha = sha.sha(data_file.read())
    data_length = data_file.tell()
    data_file.seek(0)

    block_index = dura_pb2.BlockIndex()
    block_index.MergeFromString(index_file.read())
    index_file.close()

    _log.info('block data length check: %s',
        data_length == block_index.data_length)
    _log.info('block data sha1 check: %s',
        data_sha.digest() == block_index.data_sha1)

    for file in block_index.file:
        _log.info('  file: %-60s %10d', file.path, file.data_length)
        if file.data_length == 0:
            # no content; nothing to check (and no offset recorded).
            continue
        assert data_file.tell() == file.data_offset, \
            (data_file.tell(), file.data_offset)
        body_bytes = data_file.read(file.data_length)
        assert len(body_bytes) == file.data_length
        assert sha.sha(body_bytes).digest() == file.data_sha1
