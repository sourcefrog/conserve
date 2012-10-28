# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Blocks, holding backup contents, within bands."""

import logging
import sha

from duralib.ioutils import (
    write_proto_to_file,
    )
from duralib.proto.dura_pb2 import (
    BlockIndex,
    )


from duralib import _log


def match_block_index_name(filename):
    filename = filename.lower()
    if filename.startswith('d') and filename.endswith('.i'):
        return filename[1:-2]


class BlockWriter(object):

    def __init__(self, band, block_number):
        self.band = band
        self.block_number = block_number
        assert len(block_number) == 6
        self.base_path = self.band.relpath('d' + block_number)
        self.data_path = self.base_path + '.d'
        self.index_path = self.base_path + '.i'
        self.open = False
        self.file_count = 0
        self.data_file_position = 0

    def __repr__(self):
        return '%s(%r)' % (
            self.__class__.__name__,
            self.base_path)

    def begin(self):
        assert self.open == False
        _log.debug('begin %r', self)
        self.block_index = BlockIndex()
        # TODO(mbp): Make sure not to overwrite existing files.
        self.data_file = open(self.data_path, 'wb')
        self.data_sha = sha.sha()
        self.open = True

    def finish(self):
        _log.debug('finish %r', self)
        self.block_index.data_sha1 = self.data_sha.digest()
        self.block_index.data_length = self.data_file.tell()
        self.data_file.close()
        write_proto_to_file(self.block_index, self.index_path)
        self.open = False

    def store_file(self, path, file_type, content):
        file_index = self.block_index.file.add()
        file_index.file_type = file_type
        file_index.path = path
        self.file_count += 1
        if content is not None:
            self.store_bulk_content(file_index, content)

    def store_bulk_content(self, file_index, file_content):
        file_index.data_length = body_length = len(file_content)
        if body_length:
            file_index.data_sha1 = sha.sha(file_content).digest()
            file_index.data_offset = self.data_file.tell()
        self.data_file.write(file_content)
        self.data_sha.update(file_content)
        self.data_file_position = self.data_file.tell()