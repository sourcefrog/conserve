# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Backup files into the archive."""

import logging
import os
import stat
import sys

from duralib.proto import dura_pb2


_log = logging.getLogger('dura')


def do_backup(file_names, to_archive):
    band = to_archive.create_band()
    store_files(file_names, band)
    band.finish_band()


def store_files(file_names, to_band):
    """Write some files into a band.

    Args:
        file_names [str]: Sequence of file names to store.
        to_band (Band): Band object, already existing and open for
            writing.
    """
    # TODO(mbp): Split across multiple blocks
    block_writer = to_band.create_block()
    i_file = 0

    for file_name in file_names:
        i_file += 1
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
        block_writer.store_file(file_name, ptype, file_content)

        if i_file % 20 == 0:
            _log.debug("starting new block after %d files", i_file)
            block_writer.finish()
            block_writer = to_band.create_block()

    block_writer.finish()

    # TODO(mbp): Maybe also store the as-compressed md5 so that we can check it
    # against a hash provided by the storage system, without reading back the
    # whole thing?

