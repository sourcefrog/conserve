# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line tests"""

import errno
import os.path
import tempfile
import unittest

import testresources

from duralib import archive
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
