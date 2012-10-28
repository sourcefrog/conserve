# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Test fixtures for Dura.

See <http://pypi.python.org/pypi/testfixtures>
"""

# Named this way because absolute import seems to clash with python-coverage.

from __future__ import absolute_import

import os.path
import unittest

import fixtures

from duralib.archive import Archive


class EmptyArchive(fixtures.Fixture):
    """Create an empty writable archive."""

    def setUp(self):
        super(EmptyArchive, self).setUp()
        self._tmpdir_fixture = self.useFixture(fixtures.TempDir())
        self.archive = Archive.create(os.path.join(
            self._tmpdir_fixture.path, "testarchive"))

    # No need for a tearDown: deleting the underlying tmpdir is enough.


class PopulatedArchive(fixtures.Fixture):

    def setUp(self):
        super(PopulatedArchive, self).setUp()
        self.archive = self.useFixture(EmptyArchive()).archive
        n_bands = 3
        for i in range(n_bands):
            # TODO(mbp): Actually store some files?
            self.archive.create_band().finish_band()