# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Base errors."""

class DuraError(StandardError):

    def __init__(self, **kwargs):
        self.kwargs = kwargs
        # Greedily format, to make sure that the args actually match the
        # format string.
        self._str = self._fmt % kwargs

    def __str__(self):
        return self._str