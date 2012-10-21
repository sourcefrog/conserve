#! /usr/bin/python3
# Copyright 2012 Martin Pool

import gettext
import logging
import os
import sys


def main(argv):
    logging.basicConfig(level=logging.DEBUG)
    gettext.install('myapplication', '/usr/share/locale', unicode=1)
    print _("hello!")
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
