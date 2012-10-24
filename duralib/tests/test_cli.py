# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line tests"""

import errno
import os.path
import tempfile
import unittest

import testresources

from duralib.archive import Archive
from duralib import cli
from duralib.tests.resources import TemporaryDirectory


class TestCommandLine(testresources.ResourcedTestCase):

    resources = [('tmpdir', TemporaryDirectory())]

    def subpath(self, p):
        return os.path.join(self.tmpdir, p)

    def test_create_archive(self):
        cli.run_command(['create-archive', self.subpath('a')])
        self.assertTrue(os.path.isfile(
            os.path.join(self.subpath('a'), 'DURA-ARCHIVE')))

    def test_backup(self):
        Archive.create(self.subpath('a'))
        source_path = self.subpath('sourcefile')
        file(source_path, 'w').write('hello!')
        cli.run_command(['backup', source_path, 'a'])
        # TODO(mbp): Check something was actually written?  How?
        # Maybe look that there's now one band.