#! /usr/bin/python2
# Copyright 2012 Martin Pool

import gettext
import logging
import os
import sys

from duralib import cli


def main(argv):
    logging.basicConfig(level=logging.DEBUG)
    gettext.install('dura', '/usr/share/locale', unicode=1)
    return cli.run_command(argv[1:])


if __name__ == '__main__':
    sys.exit(main(sys.argv))
