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

## Running the tests

`cargo test` runs all the tests that are fast and have no external dependencies.
This is enough for normal iterative development. There are also some special tests.

`cargo test -- --include-ignored` runs some property-based tests that are slower (taking about a minute).

`cargo test --features fail/failpoints` runs tests that use Rust [failpoints](https://docs.rs/fail/) to exercise error handling.

`cargo test --features s3-integration-tests` runs tests that create real AWS S3 buckets and objects. These tests are slow and require AWS credentials to be set up. The buckets should be automatically cleaned up after the tests, but be aware that they are real resources and may incur (small) costs. I'd recommend you run this with credentials for an account that doesn't contain any important data, just in case.

## Adding new tests

It's important that Conserve is well tested. However, adding tests can sometimes
be harder than writing the code itself, especially for complex or subtle tests
or if you're new to the codebase. So feel free to ask for help or to put up a PR
that is not yet well tested.

It's also important that the tests are very deterministic and hermetic.

Code can be tested in any of five different places:

- API tests, as "unit tests" within the implementation modules. Focus tests on
  using the public API to test the implementation, although it's OK to call
  private functions if that's the most practical way to test something important.

- Black-box tests that run the Conserve CLI as a binary, in `tests/cli`. All the
  main uses of the CLI should be exercised here. However, running the binary is
  somewhat slower than calling functions directly.

- Time-consuming tests, using proptest, in `tests/expensive`. These are
  `#[ignore]` at the test level, and only run by
  `cargo test -- --include-ignored`, which is done on CI.

- Doctests, especially for functions in the public API that are amenable to
  small examples. These are somewhat slow to build so are used only in cases
  where an example is especially helpful in describing the API.

- API integration tests for things that don't fit well in a single module.

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
