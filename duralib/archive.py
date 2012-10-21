# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""dura archive format marker.

There is a json file 'format' in the root of every archive; this
class reads and writes it.
"""

import errno
import os.path
import time

from google.protobuf import text_format
from google.protobuf.message import DecodeError

from duralib import errors
from duralib.proto import dura_pb2


ARCHIVE_HEADER_NAME = "DURA-ARCHIVE"
_HEADER_MAGIC = "dura backup archive"


class Archive(object):
    """Backup archive: holds backup versions.

    An Archive object corresponds to an archive on disk
    holding an archive header plus a number of backup
    versions.  All the versions should typically be
    copies of a single directory.
    """

    @classmethod
    def create(cls, path):
        """Create a new archive.

        The archive is created as a new directory.
        """
        os.mkdir(path)
        new_archive = cls(path)
        with file(new_archive._header_path, 'wb') as header_file:
            header_file.write(new_archive._make_archive_header_bytestring())
        return new_archive

    @classmethod
    def open(cls, path):
        new_archive = cls(path)
        new_archive._check_header()
        return new_archive

    def __init__(self, path):
        """Construct an Archive instance."""
        self.path = path
        self._header_path = os.path.join(self.path, ARCHIVE_HEADER_NAME)

    def _check_header(self):
        try:
            with file(self._header_path, 'rb') as header_file:
                header_bytes = header_file.read()
        except IOError as e:
            if e.errno == errno.ENOENT:
                raise NoSuchArchive(path=self._header_path, error=e)
            else:
                # TODO(mbp): Other wrappers?
                raise
        # check contents
        header = dura_pb2.ArchiveHeader()
        try:
            header.ParseFromString(header_bytes)
        except DecodeError:
            raise BadArchiveHeader(header_path=self._header_path)
        if header.magic != _HEADER_MAGIC:
            raise BadArchiveHeader(header_path=self._header_path)

    def _make_archive_header_bytestring(self):
        """Make archive header binary protobuf message.
        """
        header = dura_pb2.ArchiveHeader()
        header.magic = _HEADER_MAGIC
        return header.SerializeToString()


class NoSuchArchive(errors.DuraError):

    _fmt = "No such archive: %(path)s: %(error)s"


class BadArchiveHeader(errors.DuraError):

    _fmt = "Bad archive header: %(header_path)s"