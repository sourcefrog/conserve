# Cram tests for Conserve

This directory contains black-box tests based on Cram <https://bitheap.org/cram/>.

The tests should be run with

    CRAM_OPTIONS="--indent=4 -v"

The conserve executable must be on the path.

TODO: Automate something like

    PATH=$bindir:$PATH cram $CRAM_OPTIONS -i *.md

Many of these tests currently fail because of unimplemented functions or
different behavior in the Rust version.
