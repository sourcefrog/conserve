# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Unit test Archive marker/metadata"""


from __future__ import absolute_import

import errno
import os.path
import tempfile
import unittest

from fixtures import TempDir, TestWithFixtures
from testresources import ResourcedTestCase

from duralib.archive import (
    Archive,
    BadArchiveHeader,
    NoSuchArchive,
    )
from duralib.tests.base import DuraTestCase
from duralib.tests.fixtures import EmptyArchive


class TestArchive(DuraTestCase):

    def setUp(self):
        super(TestArchive, self).setUp()
        self.archive_path = self.subpath("testarchive")

    def test_create_archive(self):
        new_archive = Archive.create(self.archive_path)
        self.assertEquals(self.archive_path, new_archive.path)
        self.assertTrue(os.path.isdir(self.archive_path))
        self.assertTrue(
            os.path.isfile(
                os.path.join(self.archive_path, "DURA-ARCHIVE")))

    def test_reopen_archive(self):
        new_archive = Archive.create(self.archive_path)
        second = Archive.open(self.archive_path)
        self.assertEquals(self.archive_path, second.path)

    def test_open_nonexistent(self):
        # Don't create it
        with self.assertRaises(NoSuchArchive) as ar:
            Archive.open(self.archive_path)
        self.assertRegexpMatches(str(ar.exception),
            r"No such archive: .*testarchive.*%s"
            % os.strerror(errno.ENOENT))

    def test_open_bad_magic(self):
        orig_archive = Archive.create(self.archive_path)
        with file(orig_archive._header_path, "wb") as f:
            f.write("not this!")
        with self.assertRaises(BadArchiveHeader) as ar:
            Archive.open(self.archive_path)
        self.assertEquals(
            "Bad archive header: %s" % orig_archive._header_path,
            str(ar.exception))

