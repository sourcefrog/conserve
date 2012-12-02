# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").
#
# vim: et sw=4

"""Unit test validation.

Validation is mostly tested against a variety of scenarios of valid and
(mostly) invalid archives, to see that it does detect corruption in the 
right way.
"""


from __future__ import absolute_import

import errno
import os.path
import unittest

from duralib.archive import (
    Archive,
    BadArchiveHeader,
    NoSuchArchive,
    )
from duralib.ioutils import (
    write_proto_to_file,
    )
from duralib.proto.dura_pb2 import (
    ArchiveHeader,
    )
from duralib.validate import validate_archive

from duralib.tests.base import DuraTestCase
from duralib.tests.durafixtures import (
    EmptyArchive,
    PopulatedArchive,
    )


class TestValidate(DuraTestCase):

    def test_validate_empty(self):
        """Validate a clean empty archive."""
        archive = self.useFixture(EmptyArchive()).archive
        validate_archive(archive.path)

        # TODO(mbp): Use some kind of ui abstraction so that we can observe what
        # was claimed to be validated.
        