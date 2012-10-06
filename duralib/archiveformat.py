# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""dura archive format marker.

There is a json file 'format' in the root of every archive; this 
class reads and writes it.
"""

import json
import time

class ArchiveFormat(object):

    @classmethod
    def create(cls):
        self = cls()
        self.version = 1
        self.optional_read_flags = {}
        self.optional_write_flags = {}
        self.mandatory_read_flags = {}
        self.mandatory_write_flags = {}
        self.created_unixtime = time.time()
        return self

    def as_json(self):
        """Return a json string representation."""
        format_dict = dict(
            dura_backup_version=self.version,
            optional_read_flags=self.optional_read_flags,
            optional_write_flags=self.optional_write_flags,
            mandatory_read_flags=self.mandatory_read_flags,
            mandatory_write_flags=self.mandatory_write_flags,
            created_unixtime=self.created_unixtime,
            )
        return json.dumps(format_dict, sort_keys=True, indent=2)


if __name__ == '__main__':
    print(ArchiveFormat.create().as_json())
