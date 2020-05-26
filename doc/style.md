# Conserve code style guide

## Naming

<https://rust-lang.github.io/api-guidelines/naming.html>

Not all of the existing code is consistent in how it names things. Here is the
new pattern.

Things that read are `*Reader`: `IndexReader`, `BlockReader`. Things that write
`*Writer`.

Counts of work done return `*Stats` particular to the type, for example
`IndexWriterStats`. This may be returned from one-shot methods, or extracted
from the object by its `finish` method.

Objects that need to be explicitly finished (to write a tail file or to flush)
have a `.finish()` method, which should consume the object. If the object has
accumulating stats, they are returned from `finalize()`.

To open an existing object, call `.open()` on the class, and this constructs and
returns the corresponding `Reader` or `Writer`. Typically the first parameter is
the corresponding parent object, except for the Archive or LocalTree, which can
be constructed from a filename. (Although, in future, from a `Transport`.)

To make a new one, `.create()` which returns a `Writer`.

TODO: Split `Band` into `BandReader` and `BandWriter`.

TODO: Unify `StoreFiles` into `BandWriter`, probably.

## Messages

Error/log messages start with a capital but have no trailing period.

## Stats

Within stats objects, the last word of the name is the unit of measurement, eg
`deduplicated_bytes`, `deduplicated_blocks`.
