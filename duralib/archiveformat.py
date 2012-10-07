# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""dura archive format marker.

There is a json file 'format' in the root of every archive; this 
class reads and writes it.
"""

import json
import time
from google.protobuf import text_format

from duralib.proto import dura_pb2

class ArchiveFormat(object):

    @classmethod
    def create(cls):
        self = cls()
        return self

    def as_pb2_ascii(self):
        header = dura_pb2.ArchiveHeader()
        header.magic = "dura archive"
        header.read_version = 0
        header.write_version = 0
        return text_format.MessageToString(header)


if __name__ == '__main__':
    print(ArchiveFormat.create().as_pb2_ascii())
