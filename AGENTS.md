# Rust Development Guide

## Commands

```cmd
# Checks for compilation errors and linting warnings (Fast)
make check

# Runs all unit and integration tests (Slow)
make test

# Runs a specific test, module, or test group (Fast)
make test my_test_case

# Runs all benchmark performance tests (Very Slow)
make bench
```

## Restrictions

- Do not modify `profile.*` in `Cargo.toml`.
- Avoid unsafe commands `cargo test` as it has no protection against infinite loops and memory leaks.
- Avoid unsafe blocks `unsafe` unless absolutely necessary for FFI or extreme hot-path optimization; must be documented and audited.
- Always declare dependencies at the top of the file. This maintains a clear dependency graph and avoids Qualified Paths in the logic.
