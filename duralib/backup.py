# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Backup files into the archive."""

import logging
import os
import sha
import stat
import sys

from duralib.proto import dura_pb2


_log = logging.getLogger('dura')


def store_files(file_names, to_band):
    """Write some files into a band.

    Args:
        file_names [str]: Sequence of file names to store.
        to_band (Band): Band object, already existing and open for
            writing.
    """
    # TODO(mbp): Don't overwrite existing files.
    # TODO(mbp): Split across multiple blocks
    block_number = '000000'
    base_name = to_band.relpath('d' + block_number)
    data_file = open(base_name + '.d', 'wb')
    index_file = open(base_name + '.i', 'wb')
    data_sha = sha.sha()

    block_index = dura_pb2.BlockIndex()
    for file_name in file_names:
        st = os.lstat(file_name)
        _log.info('store %s' % file_name)

        if stat.S_ISREG(st.st_mode):
            ptype = dura_pb2.REGULAR
            # TODO(mbp): stream content for large files
            file_content = open(file_name).read()
        elif stat.S_ISDIR(st.st_mode):
            ptype = dura_pb2.DIRECTORY
            file_content = None
        elif stat.S_ISLNK(st.st_mode):
            ptype = dura_pb2.SYMLINK
            # TODO(mbp): Fix the race here between discovering it's a link,
            # and trying to read it.
            file_content = os.readlink(file_name)
        else:
            # TODO(mbp): Maybe eventually store devices etc too
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

