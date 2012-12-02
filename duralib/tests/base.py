# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Base test case for Dura"""


from __future__ import absolute_import

import os.path
import unittest

from fixtures import (
    Fixture,
    MonkeyPatch,
    TempDir,
    TestWithFixtures,
    )

from duralib import ui


class DuraTestCase(TestWithFixtures):
    """Common facilities for all Dura tests.

    Attributes:
      tmpdir: str -- path of general purpose temporary directory
    """

    def setUp(self):
        super(DuraTestCase, self).setUp()
        self.tmpdir = self.useFixture(TempDir()).path
        self.capture_ui = CaptureUI()
        self.useFixture(self.capture_ui)

    def relpath(self, p):
        """Make a path relative to tmpdir."""
        return os.path.join(self.tmpdir, p)


class CaptureUI(Fixture):
    """Intercept and record all UI actions."""

    # TODO(mbp): Maybe a structured way to check for ui actions, skipping actions 
    # or attributes that don't matter.  Or maybe we should just test them.

    def setUp(self):
        super(CaptureUI, self).setUp()
        self.actions = []
        self.useFixture(MonkeyPatch('duralib.ui.emit', self.captured_emit))

    def captured_emit(self, action, **kwargs):
        self.actions.append((action, kwargs.copy()))