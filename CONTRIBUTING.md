# Contributing to Conserve

Contributions are very welcome.

If your change is nontrivial please talk to me in a bug about the approach
before writing lots of code.

By sending patches or pull requests, you consent for your code to be licensed
under the existing Conserve licence, the GNU GPL v2.

## Where to begin

There are some bugs marked [good first issue][] if you'd like to contribute but
don't have a particular feature or bug in mind.

[good first issue]:
  https://github.com/sourcefrog/conserve/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22

## Communication

Please feel free to comment on a bug if you're interested in working on it, or
to send a draft pull request for feedback before it's ready to merge.

## Code style

Beyond general Rust style, there is a [Conserve style guide](doc/style.md)
covering items such as naming. The existing code does not all comply to it, but
it sets a direction.

## Testing

Code can be tested in any of five different places:

- Public API tests, in `tests/api`. All core features should be exercised here,
  although some edge cases that can't be reached through the public API could be
  left for unit tests.

- Black-box tests that run the Conserve CLI as a binary, in `tests/cli`. All the
  main uses of the CLI should be exercised here.

- Time-consuming tests, using proptest, in `tests/expensive`. These are
  `#[ignore]` at the test level, and only run by
  `cargo test -- --include-ignored`, which is done on CI.

- Doctests, especially for functions in the public API that are amenable to
  small examples. These are somewhat slow to build so are used only in cases
  where an example is especially helpful in describing the API.

- Individual functions, as unit tests within the implementation module. Prefer
  this for things that are important to test, but not exposed or not easily
  testable through the public API. Most things should be tested through the
  public API and not inside the implementation.

If it's hard to work out how to test your change please feel free to put up a
draft PR with no tests and just ask.

### Archive snapshots

There are some snapshots in `testdata/archive` of archives written by previous
versions of Conserve, to test cross-version compatibility.

A new snapshot should be added in every release that makes an archive format
change, even changes that are expected to be very small or to be fully backwards
and forwards compatible.

Existing snapshots should generally never be changed.

New features that depend on new archive fields should be tested against old
archive snapshots to ensure they are handled gracefully.
