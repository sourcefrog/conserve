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


LOG = logging.getLogger('dura.band')


def write_band(file_names, to_filename):
    data_file = open(to_filename + '.d', 'wb')
    index_file = open(to_filename + '.i', 'wb')
    data_sha = sha.sha()

    block_index = dura_pb2.BlockIndex()
    for file_name in file_names:
        st = os.stat(file_name)

        if stat.S_ISREG(st.st_mode):
            ptype = dura_pb2.FileIndex.REGULAR
        else:
            # TODO(mbp): For symlinks, body should be the readlink.
            LOG.warning("skipping non-regular file %r", file_name)
            continue

        # TODO(mbp): stream content for large files
        file_content = open(file_name).read()

        file_index = block_index.file.add()
        file_index.file_type = ptype
        file_index.path = file_name
        file_index.body_length = body_length = len(file_content)
        if body_length:
            file_index.sha1_hash = sha.sha(file_content).digest()
            file_index.body_data_start = data_file.tell()

        data_file.write(file_content)
        data_sha.update(file_content)

    block_index.data_sha1 = data_sha.digest()
    block_index.data_length = data_file.tell()
    index_file.write(block_index.SerializeToString())

    data_file.close()
    index_file.close()

    logging.debug("band index:\n%s", text_format.MessageToString(block_index))
    

if __name__ == "__main__":
    assert len(sys.argv) >= 3
    logging.basicConfig(level=logging.DEBUG)
    file_names = sys.argv[1:-1]
    write_band(file_names, sys.argv[-1])
