# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""TestResources for Dura"""

import os.path
import shutil
import tempfile
import unittest


from testresources import TestResourceManager


class TemporaryDirectory(TestResourceManager):

    def clean(self, resource):
        shutil.rmtree(resource)

    def make(self, unused_dependencies):
        return tempfile.mkdtemp()

    def isDirty(self, resource):
        # Can't detect when the directory is written to, so assume it can never
        # be reused.  We could list the directory, but that might not catch it
        # being open as a cwd etc.
        return True

