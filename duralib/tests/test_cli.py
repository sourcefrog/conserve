# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Command-line tests"""

import errno
import os.path
import tempfile
import unittest

from duralib import archive
from duralib import cli


class TestCommandLine(unittest.TestCase):

    def setUp(self):
        self.tmpdir = tempfile.mkdtemp()
        self.archive_path = os.path.join(self.tmpdir, "testarchive")

    def test_create_archive(self):
        cli.run_command(['create-archive', self.archive_path])
        self.assertTrue(os.path.isfile(
            os.path.join(self.archive_path, 'DURA-ARCHIVE')))