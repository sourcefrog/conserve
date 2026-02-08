# Agent Guidelines

## Pre-commit Checklist

Before committing any changes to this repository, always run:

1. **`cargo fmt --check`** - Verify code is properly formatted
   - If this fails, run `cargo fmt` to format the code
2. **`cargo check`** - Ensure code compiles without errors

These commands help maintain code quality and consistency across the project.

## Additional Quality Checks

Consider also running:
- `cargo test` - Run all tests
- `cargo clippy` - Run linter for additional code quality checks
