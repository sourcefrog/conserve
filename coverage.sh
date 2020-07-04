#! /bin/sh -ex

export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Zprofile -Ccodegen-units=1 -Copt-level=0 -Clink-dead-code -Coverflow-checks=off -Zpanic_abort_tests -Cpanic=abort"
export RUSTDOCFLAGS="-Cpanic=abort"
# cargo +nightly clean
cargo +nightly test
grcov ./target/debug/ -s . -t html --llvm --branch --ignore-not-existing \
  --excl-start GRCOV_EXCLUDE_START \
  --excl-stop GRCOV_EXCLUDE_STOP \
  --excl-line GRCOV_EXCLUDE \
  -o ./target/debug/coverage/

