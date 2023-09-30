# Conserve code style guide

The style below is the current intention for Conserve. The actual code may lag
behind.

## Naming

<https://rust-lang.github.io/api-guidelines/naming.html>

Not all of the existing code is consistent in how it names things. Here is the
intended pattern for new or updated code.

### Types

Things that read are `*Reader`: `IndexReader`, `BlockReader`. Things that write
`*Writer`.

Counts of work done return `*Stats` particular to the type, for example
`IndexWriterStats`. This may be returned from one-shot methods, or extracted
from the object by its `finish` method.

TODO: Split `Band` into `BandReader` and `BandWriter`.

TODO: Unify `StoreFiles` into `BandWriter`, probably.

### Functions

Objects that need to be explicitly finished (to write a tail file or to flush)
have a `.finish()` method, which should consume the object. If the object has
accumulating stats, they are returned from `finalize()`.

To open an existing object, call `::open()` on the class, and this constructs
and returns the corresponding `Reader` or `Writer`. Typically the first
parameter is the corresponding parent object, except for the Archive or
LocalTree, which can be constructed from a filename. (Although, in future, from
a `Transport`.)

To make a new one, `::create()` which returns a `Writer`.

Versions that take a `Path` rather than a `Transport` should be called
`open_path` and `create_path`.

### Arguments

If the function takes a `Monitor` argument it should be the last.

If it takes some kind of `Options` that should be last before the monitor.

In general arguments that are conceptually inputs should be towards the left,
and those that are conceptually outputs should be towards the right.

### Variables

Local variables (not in a closure) that hold a "major" object should have a
snake-case name corresponding to the type name. For example,

```rust
    let mut progress_bar = ProgressBar::new();
```

## Messages

Error/log messages start with a capital but have no trailing period.

## Stats

All stats objects are in the `conserve::stats` module, so that they can more
easily be kept consistent with each other.

Within stats objects, the last word of the name is the unit of measurement, eg
`deduplicated_bytes`, `deduplicated_blocks`.

## Tests

### Structure

Code in Conserve can be tested in any of three ways:

1. Key features and behaviors accessible through the command-line interface
   should be tested in `tests/cli`, which runs the `conserve` binary as a
   subprocess and examines its output. Since Conserve is
   primarily intended for use as a command-line tool these are the most
   important tests to add.

2. Public API behavior is tested through `tests/api`. These are useful for
   behaviors that are harder to exercise or examine through the CLI.

3. Unit tests that require access to private interfaces live inside the source
   files. These are only needed when it's important to test something that
   should not be public.

Doc tests are discouraged because they're slower to build and run.

### Test data

Many tests need an archive or working tree as input.

Some archives are provided in the `testdata/` tree. If the archive will be
mutated by the test it should be copied to a temporary directory first.

Tests that need a source tree can build it using `assert_fs` or make use of the
example trees under `testdata/tree/`. Note that the git checkout (or Cargo build
tree) won't have deterministic permissions or mtimes.

## `use` statements

Use `use crate::xyz` rather than `use super::xyz` to import other things from
the Conserve implementation. (Either is valid and they seem just as good, but
let's pick `crate` to be consistent.)

Conserve implementation code and integration tests can say `use crate::*` to
include every re-exported symbol, although this isn't recommended for external
clients.

Unit test submodules should say `use super::*`.

Otherwise, avoid `use ...::*` except for libraries that specifically recommend
it.
