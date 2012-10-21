# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Unit test Archive marker/metadata"""

import os.path
import tempfile
import unittest

from duralib import archive


class TestArchive(unittest.TestCase):

    def setUp(self):
        self.tmpdir = tempfile.mkdtemp()
        self.archive_path = os.path.join(self.tmpdir, "testarchive")

    def test_create_archive(self):
        new_archive = archive.Archive.create(self.archive_path)
        self.assertEquals(self.archive_path, new_archive.path)
        self.assertTrue(os.path.isdir(self.archive_path))
        self.assertTrue(
            os.path.isfile(
                os.path.join(self.archive_path, "DURA-ARCHIVE")))

    def test_reopen_archive(self):
        new_archive = archive.Archive.create(self.archive_path)
        second = archive.Archive.open(self.archive_path)
        self.assertEquals(self.archive_path, second.path)
