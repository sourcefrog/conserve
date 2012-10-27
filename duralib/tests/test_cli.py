# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line tests"""


from __future__ import absolute_import

import os.path
import tempfile
import unittest

from fixtures import TempDir, TestWithFixtures

from duralib.archive import Archive
from duralib import cli
from duralib.tests.base import DuraTestCase
from duralib.tests.fixtures import EmptyArchive


class TestCommandLine(DuraTestCase):

    def test_create_archive(self):
        cli.run_command(['create-archive', self.subpath('a')])
        self.assertTrue(os.path.isfile(
            os.path.join(self.subpath('a'), 'DURA-ARCHIVE')))

    def test_describe_archive(self):
        # smoke test
        # TODO(mbp): Check the output.
        cli.run_command([
            'describe-archive', self.useFixture(EmptyArchive()).archive.path])


class TestBackupCommand(DuraTestCase):

    def test_backup(self):
        archive_fixture = self.useFixture(EmptyArchive())
        source_path = self.subpath('sourcefile')
        file(source_path, 'w').write('hello!')
        archive_path = archive_fixture.archive.path
        cli.run_command(['backup', source_path, archive_path])
        expected_band_path = os.path.join(archive_path, 'b0000')
        self.assertTrue(os.path.isdir(expected_band_path))
        self.assertEquals(
            ['d000000.d', 'd000000.i'],
            sorted(os.listdir(expected_band_path)))
