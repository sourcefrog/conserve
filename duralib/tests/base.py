# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Base test case for Dura"""


from __future__ import absolute_import

import os.path
import unittest

from fixtures import TempDir, TestWithFixtures


class DuraTestCase(TestWithFixtures):
    """Common facilities for all Dura tests.

    Attributes:
      tmpdir: str -- path of general purpose temporary directory
    """

    def setUp(self):
        super(DuraTestCase, self).setUp()
        self.tmpdir = self.useFixture(TempDir()).path

    def relpath(self, p):
        """Make a path relative to tmpdir."""
        return os.path.join(self.tmpdir, p)