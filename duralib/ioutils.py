# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""IO utilities."""

def write_proto_to_file(proto_obj, filename):
    with file(filename, 'wb') as f:
        f.write(proto_obj.SerializeToString())