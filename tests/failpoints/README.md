# Conserve failpoint tests

The tests in this directory simulate IO errors or other failures in Conserve, and assert that Conserve handles and reports them correctly.

Failpoints aren't built by default.

To run them use

    cargo test --features fail/failpoints --test failpoints
