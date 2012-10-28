# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0.

"""Print out the contents of an index entry."""

import collections

from duralib import band
from duralib.proto import dura_pb2


def bin_to_hex(s):
    return s.encode('hex_codec')
    ## return ''.join(('%02x' % ord(c)) for c in s)


file_type_map = collections.defaultdict(lambda k: '?')
file_type_map.update({
    dura_pb2.REGULAR: '.',
    dura_pb2.DIRECTORY: '/',
    dura_pb2.SYMLINK: '@',
    })


def print_block_index(block_index):
    for entry in block_index.file:
        if entry.data_sha1:
            sha_string = bin_to_hex(entry.data_sha1)
        else:
            sha_string = '-'
        print '%-40s %10d %10s %s %s' % (
            sha_string,
            entry.data_length,
            entry.data_offset,
            file_type_map[entry.file_type],
            entry.path)
    print '%s %10s %10s =' % ('=' * 40, '=' * 10, '=' * 10)
    print '%40s %10s %10d' % (
            bin_to_hex(block_index.data_sha1),
            '-',
            block_index.data_length)
