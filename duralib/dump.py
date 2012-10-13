# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0.

"""Print out the contents of an index file."""

import collections
import sys

from google.protobuf import text_format

from duralib import band
from duralib.proto import dura_pb2


def bin_to_hex(s):
    return ''.join(('%02x' % ord(c)) for c in s)


file_type_map = collections.defaultdict(lambda k: '?')
file_type_map.update({
    dura_pb2.REGULAR: '.',
    dura_pb2.DIRECTORY: '/',
    dura_pb2.SYMLINK: '@',
    })


def cmd_dump_index(index_file_names):
    for index_file_name in index_file_names:
        dump_index_block(index_file_name)


def dump_index_block(index_file_name):
    block_index = band.read_index(index_file_name)
    for file in block_index.file:
        if file.data_sha1:
            sha_string = bin_to_hex(file.data_sha1)
        else:
            sha_string = '-'
        print '%-40s %10d %10s %s %s' % (
            sha_string,
            file.data_length,
            file.data_offset,
            file_type_map[file.file_type],
            file.path)
    print '%s %10s %10s =' % ('=' * 40, '=' * 10, '=' * 10)
    print '%40s %10s %10d' % (
            bin_to_hex(block_index.data_sha1),
            '-',
            block_index.data_length)
    # TODO(mbp): Maybe have an option to just print the protobuf.


if __name__ == "__main__":
    cmd_dump_index(sys.argv[1:])

