# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line tests"""


from __future__ import absolute_import

from cStringIO import StringIO
import os.path
import tempfile
import unittest

from duralib.archive import Archive
from duralib import cli
from duralib.tests.base import DuraTestCase
from duralib.tests.durafixtures import EmptyArchive


class TestCommandLine(DuraTestCase):

    def test_create_archive(self):
        cli.run_command(['create-archive', self.relpath('a')])
        self.assertTrue(os.path.isfile(
            os.path.join(self.relpath('a'), 'DURA-ARCHIVE')))

    def test_describe_archive(self):
        # smoke test
        # TODO(mbp): Check the output.
        cli.run_command([
            'describe-archive', self.useFixture(EmptyArchive()).archive.path])


class TestBackupCommand(DuraTestCase):

    def test_backup(self):
        archive_fixture = self.useFixture(EmptyArchive())
        source_path = self.relpath('sourcefile')
        file(source_path, 'w').write('hello!')
        archive_path = archive_fixture.archive.path
        cli.run_command(['backup', source_path, archive_path])
        expected_band_path = os.path.join(archive_path, 'b0000')
        self.assertTrue(os.path.isdir(expected_band_path))
        self.assertEquals(
            ['BAND-HEAD', 'BAND-TAIL', 'd000000.d', 'd000000.i'],
            sorted(os.listdir(expected_band_path)))


def TestListBands(DuraTestCase):

    def test_list_bands(self):
        out = StringIO()
        archive = self.useFixture(PopulatedArchive()).archive
        cli.run_command(
            ['list-bands', '-q', archive.path],
            stdout=out)
        self.assertEquals(
            "\n".join("%04d" % i for i in range(3)) + "\n",
            out.getvalue())