#! /usr/bin/python3
# Copyright 2012 Martin Pool

import gettext
import logging
import os
import sys

from duralib import cli


def main(argv):
    logging.basicConfig(level=logging.DEBUG)
    gettext.install('myapplication', '/usr/share/locale', unicode=1)
    return cli.run(argv)


if __name__ == '__main__':
    sys.exit(main(sys.argv))
