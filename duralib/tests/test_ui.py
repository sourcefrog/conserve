# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Test Dura user interface and ui test support."""

from duralib import ui
from duralib.tests.base import DuraTestCase


class TestUI(DuraTestCase):

	def test_ui_emissions_captured(self):
		"""UI emissions are captured in tests."""
		ui.emit('hello')
		self.assertEquals(
			[('hello', {})],
			self.capture_ui.actions)