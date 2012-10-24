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

    def setUp(self):
        super(TestCommandLine, self).setUp()
        self.tmpdir = self.useFixture(TempDir()).path

    def test_create_archive(self):
        cli.run_command(['create-archive', self.subpath('a')])
        self.assertTrue(os.path.isfile(
            os.path.join(self.subpath('a'), 'DURA-ARCHIVE')))

    def test_describe_archive(self):
        # smoke test
        cli.run_command([
            'describe-archive', self.useFixture(EmptyArchive()).archive.path])


class TestBackupCommand(DuraTestCase):

    def test_backup(self):
        archive_fixture = self.useFixture(EmptyArchive())
        source_path = self.subpath('sourcefile')
        file(source_path, 'w').write('hello!')
        cli.run_command(['backup', source_path, archive_fixture.archive.path])
        # TODO(mbp): Check something was actually written?  How?
        # Maybe look that there's now one band.