# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Dura user interface abstraction layer"""


from duralib import _log


def emit(action_name, **kwargs):
    _log.info('%s %r', action_name, kwargs)