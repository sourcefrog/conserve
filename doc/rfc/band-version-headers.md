# Minimum versions on band headers

<https://github.com/sourcefrog/conserve/issues/96>

## Summary

Extend the band header to include the minimum Conserve version that can read
this band. Older versions will cleanly decline to read bands they won't
understand.

## Background

As I write this in 2020-02, Conserve has been on the 0.6 archive format for
about a year, and it seems to be holding up well. At a high level this format
says there is a shared blockdir, hashed by BLAKE2 and compressed by Stubby;
bands named a certain way; and some overall structure.

## Motivation

I have made some changes to the index format, including skipping serialization
of some fields when empty, and adding an mtime nanoseconds field. Similar
changes are foreseeable in future, including addition of permissions and
ownership, files starting at a >0 offset within their block, and band tails
giving a count of blocks (https://github.com/sourcefrog/conserve/issues/70).

For all of these changes, it's easy to have new Conserve formats read the old
format: typically the old format is a subset of the current format.

Older software, reading indexes written by the newer version, will skip fields
they don't understand,
[which is the default Serde behavior](https://serde.rs/container-attrs.html#deny_unknown_fields).
For cases such as fractional second mtimes, or permissions, this will be fine.

For cases where we stopped emitting some fields, it may error. For example,
Conserve `v0.6.0` attempting to read a band written by Conserve
`v0.6.2-12-g5700d2b` says, many times

    conserve error: Failed to deserialize index hunk "/Users/mbp/Backup/src.c6/b0068/i/00000/000000064"
    conserve error:   caused by: missing field `start` at line 1 column 255
    conserve error: Failed to deserialize index hunk "/Users/mbp/Backup/src.c6/b0068/i/00000/000000065"
    conserve error:   caused by: missing field `start` at line 1 column 320

## Design

As well as an overall archive version, add a minimum version in each band
header, and check it when the band is opened. Decline to open bands from newer
versions.

The minimum band version doesn't need to change on every release, only when the
index format changes in a way that old versions won't read it correctly.

## Alternatives

### Just use the archive version

Rather than adding a per-band version, we could bump the archive version every
time there is any change. But, this would be fairly disruptive: it requires a
new full backup (including space to make such), and it stops the newer code from
being used to access the old backups.

### Do nothing

Perhaps, using an old version of Conserve is rare, and perhaps users would take
the first debugging step of updating without being explicitly guided.

### Make all changes such that old versions will do the right thing

This seems hard in general.

### Fall back to the latest band that can be read

Conceivably, an old version could "do its best" by finding a version it can
read. But this seems like unnecessary complexity, because using an old build is
a rare case, and the user can presumably just update.
