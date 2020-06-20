# Contributing to Conserve

Contributions are very welcome.

By sending patches or pull requests, you consent for your code to be licensed
under the existing Conserve licence, the GNU GPL v2.

## Where to begin

There are some bugs marked [good first issue][] if you'd like to contribute but
don't have a particular feature or bug in mind.

[good first issue]: https://github.com/sourcefrog/conserve/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22

## Communication

Please feel free to comment on a bug if you're interested in working on it, or
to send a draft pull request for feedback before it's ready to merge. 

## Code style

Beyond general Rust style, there is a [Conserve style guide](doc/style.md)
covering items such as naming. The existing code does not all comply to it, but
it sets a direction.

## Testing

Code can be tested in up to four different ways:

* Individual functions, as unit tests within the implementation module. Prefer
  this for things that are important to test, but not exposed or not easily
  testable through the public API.

* Doctests, especially for functions in the public API that are amenable to
  small examples.

* Public API tests, in `tests/blackbox.rs`. All core features should be
  exercise here, although some edge cases that can't be reached through the
  public API could be left for unit tests.

* Black-box tests that run the Conserve CLI as a binary, in
  `tests/blackbox.md`.  All the main uses of the CLI should be exercised here.

If it's hard to work out how to test your change please feel free to put up a
draft PR with no tests and just ask.
