# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""IO utilities."""

import logging


_log = logging.getLogger('dura')


def write_proto_to_file(proto_obj, filename):
    proto_bytes = proto_obj.SerializeToString()
    with file(filename, 'wb') as f:
        f.write(proto_bytes)

def read_proto_from_file(cls, filename):
    pb = cls()
    try:
        with file(filename, 'rb') as f:
            file_bytes = f.read()
    except IOError as e:
        _log.warning('failed to read %s from %r: %s' % (
            cls.__name__, filename, e.strerror))
        return None
    pb.ParseFromString(file_bytes)
    return pb