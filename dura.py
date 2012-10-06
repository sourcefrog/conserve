#! /usr/bin/python3
# Copyright 2012 Martin Pool

import demjson
import logging
import os
import sys
import tarfile


def write_block(file_names, to_name):
    """Write out a block of a few files.
    """
    with tarfile.open(name=to_name, mode='w:gz') as tar:
        for filename in file_names:
            logging.info("emit file %r", filename)
            # TODO(mbp): hash the file as its read in
            tar.add(filename, recursive=False)
    logging.info("finished tar %r", to_name)


def main(argv):
    logging.basicConfig(level=logging.DEBUG)
    tarfile_name = sys.argv[-1]
    if False and os.path.exists(tarfile_name):
        logging.error("output file already exists: %s", tarfile_name)
        return 1
    if not tarfile_name.endswith(".tgz"):
        # Just a safety check not to overwrite things.
        logging.error("output file %r doesn't end in .tgz", tarfile_name)
        return 1
    write_block(sys.argv[1:-1], tarfile_name)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
