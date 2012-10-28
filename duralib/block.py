# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Blocks, holding backup contents, within bands."""

def match_block_index_name(filename):
    filename = filename.lower()
    if filename.startswith('d') and filename.endswith('.i'):
        return filename[1:-2]