# Versioning

Conserve software releases and formats are identified by the same `x.y.z`
version timeline.

## Pre-1.0 releases

Archives, and versions within archives, include a marker of their format
version.

Archives written by 0.x.y can be read and written by any 0.x.z.

Bands within an archive written by 0.x.y can only be read by 0.x.z when z >= y.

## APIs

At least prior to 1.0, there are no promises of stability for the library API.

## Command line

Additions of new flags or commands to the command line interface will be
signaled by a new minor version.
