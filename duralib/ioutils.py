# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""IO utilities."""

def write_proto_to_file(proto_obj, filename):
    proto_bytes = proto_obj.SerializeToString()
    with file(filename, 'wb') as f:
        f.write(proto_bytes)

def read_proto_from_file(cls, filename):
    pb = cls()
    with file(filename, 'rb') as f:
        file_bytes = f.read()
    pb.ParseFromString(file_bytes)
    return pb