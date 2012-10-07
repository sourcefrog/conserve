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


def make_archive_header():
    """Make archive header pb2 message.
    """
    header = dura_pb2.ArchiveHeader()
    header.magic = "dura archive"
    header.read_version = 0
    header.write_version = 0
    return header    


if __name__ == '__main__':
    print(make_archive_header().SerializeToString())
