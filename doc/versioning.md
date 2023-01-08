# Versioning

This document outlines the general approach and intentions for versioning and compatibility in Conserve.

Bear in mind that this is a hobby project and there is no guarantee whatsoever of support, correctness,
or fitness for any purpose.

## Goals

Backups written today (with at least Conserve 0.6) should be read (correctly!) by Conserve releases years into the future.

Conserve has a general goal that it should never rewrite, and so risk damaging, existing data in an archive.
Therefore, versioning cannot rely on upgrading archives.

Conserve's format today is fairly good but there is still scope and a desire to improve it by adding
features (e.g. encryption) and improving existing features (e.g. better encryption or serialization.)

Archives may become large, holding many versions of many files. Copying the whole archive might use a lot of time
and disk space, and this should be avoided. Therefore, it's strongly preferable that new format features be introduced
on a per-backup basis. Although there is a `conserve_archive_version: "0.6"` marker in the archive header my 
goal is that it will never again need to be incremented.

Command-line semantics may be relied upon by scripts. Command line arguments should not change behavior 
in a way that would foreseeably break scripts. It may be OK to deprecate and eventually remove command-line
options if the case is well justified.

## Non-goals

I postulate that it's always relatively easy to install an old or new Rust toolchain, and to build
or install a new Conserve binary. Therefore, there is not much likelihood anyone will need to use an 
old Conserve release to read newer backups, and therefore there is no need to constrain new 
releases to write old formats. 

Although Conserve can be built as a library, the library API is currently considered private to the `conserve` binary, and 
there are no promises of stability for the library API.

## Versioning strategy

Conserve release versioning is primarily time-based: after 0.6.16 releases will be numbered `YY.MM`, with no leading zeros on the month,
as described in <https://calver.org/>.

Patch releases are expected to be rare but may be numbered as `YY.MM.pp`.

Backup band headers include the minimum Conserve version needed to correctly read and validate the band.
This does _not_ need to be bumped if there is additional data that old versions can safely ignore:
for example, old versions that don't understand file owners can simply ignore them.

## Testing

It's important that new versions be able to read the archives of old versions, so the test suite includes copies of archives written by old Conserve binaries.

