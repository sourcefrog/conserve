# Remove Report objects

## Summary

Conserve 0.6 has a `Report` object that serves several purposes, fairly well but
not perfectly. It should be rethought.

## Background

`Report` does a few things together:

- Acts as a UI facade to print errors, which must be interleaved correctly with
  a terminal progress bar.

- Counts progress

- Times the duration of operations. The idea was to use this for some built-in
  profiling, but it doesn't work very well when operations are multi-threaded,
  as they increasingly are. Perhaps it's better to just use system profilers.
