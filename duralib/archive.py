# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""dura Archive object: the top-level object that holds backup data.

Within the archive there is:

 - format marker, confirming this is an archive.
 - zero or more bands, holding backup data.
"""

import errno
import os.path

from google.protobuf.message import DecodeError

from duralib import errors
from duralib.proto import dura_pb2
from duralib.band import (
    Band,
    cmp_band_numbers,
    )


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
        new_archive._write_header()
        return new_archive

    @classmethod
    def open(cls, path):
        new_archive = cls(path)
        new_archive._check_header()
        return new_archive

    def relpath(self, p):
        return os.path.join(self.path, p)

    def __init__(self, path):
        """Construct an Archive instance."""
        self.path = path
        self._header_path = os.path.join(self.path, ARCHIVE_HEADER_NAME)

    def __repr__(self):
        return '%s(%r)' % (
            self.__class__.__name__,
            getattr(self, 'path'))

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

    def _write_header(self):
        with file(self._header_path, 'wb') as header_file:
            header_file.write(_make_archive_header_bytestring())

    def create_band(self):
        """Make a new band within the archive.

        Returns:
          A new Band object, which is on disk and empty.
        """
        existing_bands = list(self.list_bands())
        if existing_bands:
            next_number = max(int(b) for b in existing_bands) + 1
        else:
            next_number = 0
        band = Band(self, str(next_number))
        band.create_directory()
        return band

    def list_bands(self):
        """Return sorted list of existing band numbers.

        Bands are sorted numerically.

        Returns:
          A list of strings, in sorted arbitrary order, each of which
          is a band number like '0042'.
        """
        result = []
        for name in os.listdir(self.path):
            band_number = Band.match_band_name(name)
            if band_number is not None:
                result.append(band_number)
        result.sort(cmp=cmp_band_numbers)
        return result


class NoSuchArchive(errors.DuraError):

    _fmt = "No such archive: %(path)s: %(error)s"


class BadArchiveHeader(errors.DuraError):

    _fmt = "Bad archive header: %(header_path)s"


def _make_archive_header_bytestring():
    """Make archive header binary protobuf message.
    """
    header = dura_pb2.ArchiveHeader()
    header.magic = _HEADER_MAGIC
    return header.SerializeToString()
