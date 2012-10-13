# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Write a band of files to the archive."""

import logging
import os
import sha
import stat
import sys

from google.protobuf import text_format

from duralib.proto import dura_pb2


LOG = logging.getLogger('dura')


def read_index(index_file_name):
    with open(index_file_name, 'rb') as index_file:
        block_index = dura_pb2.BlockIndex()
        block_index.ParseFromString(index_file.read())
        return block_index


def write_band(file_names, to_filename):
    data_file = open(to_filename + '.d', 'wb')
    index_file = open(to_filename + '.i', 'wb')
    data_sha = sha.sha()

    block_index = dura_pb2.BlockIndex()
    for file_name in file_names:
        st = os.lstat(file_name)

        LOG.info('store %s' % file_name)

        if stat.S_ISREG(st.st_mode):
            ptype = dura_pb2.FileIndex.REGULAR
            # TODO(mbp): stream content for large files
            file_content = open(file_name).read()
        elif stat.S_ISDIR(st.st_mode):
            ptype = dura_pb2.FileIndex.DIRECTORY
            file_content = None
        elif stat.S_ISLNK(st.st_mode):
            ptype = dura_pb2.FileIndex.SYMLINK
            # TODO(mbp): Race here between discovering it's a link and trying to read it.
            file_content = os.readlink(file_name)
        else:
            # TODO(mbp): For symlinks, body should be the readlink.
            LOG.warning("skipping non-regular file %r", file_name)
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

    # TODO(mbp): Maybe also store the compressed sha1 so that we can check it
    # against a hash provided by the storage system, without reading back the
    # whole thing?

    # LOG.debug("band index:\n%s", text_format.MessageToString(block_index))
    

if __name__ == "__main__":
    assert len(sys.argv) >= 3
    logging.basicConfig(level=logging.DEBUG)
    file_names = sys.argv[1:-1]
    write_band(file_names, sys.argv[-1])
