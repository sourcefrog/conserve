# Remove Report objects

## Summary

Conserve 0.6 has a `Report` object that serves several purposes, fairly well but
not perfectly. It should be rethought.

## Background

`Report` does a few things together:

- Acts as a UI facade to print errors, which must be interleaved correctly with
  a terminal progress bar.

- Counts progress

Historically it also recorded the duration of operations, but this is now gone.
It does record the overall elapsed time but no more. Detailed measurement is
probably better done in system profilers.

## Alternatives

- Return operation-specific stats, that know how to format themselves as text,
  and that are returned from the operation on success.

- Remove `HasReport`?

- Explicitly create progress bars from functions that will take a long time.

- Remove string-addressed `counts` and `sizes`.
