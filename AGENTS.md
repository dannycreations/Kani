# Kani Development Guide

## Commands

| Command                         | What it does                              | Speed                     |
| ------------------------------- | ----------------------------------------- | ------------------------- |
| `make check`                    | Compiles and lints the entire project     | Slow                      |
| `make check -- -p package_name` | Compiles and lints one specific package   | Fast                      |
| `make test`                     | Runs all tests in the entire project      | Very slow                 |
| `make test -- -p package_name`  | Runs all tests in one specific package    | Slow                      |
| `make test my_test_case`        | Runs a single test, module, or test group | Fast                      |
| `make bench`                    | Runs all performance benchmarks           | Very slow — use sparingly |

---

## Guidelines

### 1. Never edit `[profile.*]` sections in `Cargo.toml`

Sections like `[profile.dev]` and `[profile.release]` control compiler optimization settings for the whole project. They are locked because changing them can silently break **reproducibility** — the guarantee that the same code always produces the same build, on any machine, at any time.

```toml
# ✅ Allowed — adding a new dependency
[dependencies]
serde = { version = "1", features = ["derive"] }

# ❌ Forbidden — modifying any [profile.*] block
[profile.release]
opt-level = 3
```

---

### 2. Use `make test`, not `cargo test`

`make test` includes safety protections that `cargo test` does not — for example, automatic timeouts that stop a test run if it hangs. Running `cargo test` directly skips these protections entirely.

```sh
# ✅ Correct
make test
make test -- -p package_name
make test my_test_case

# ❌ Forbidden — bypasses timeout and safety protections
cargo test
cargo test -p package_name
cargo test my_test_case
```

---

### 3. Avoid `unsafe` code

`unsafe` blocks disable Rust's normal compile-time safety checks. Only use `unsafe` in these two cases:

- **FFI (Foreign Function Interface):** interacting with non-Rust code, such as a C library.
- **Performance-critical code:** only after profiling has proven a measurable speed benefit.

Every `unsafe` block **must** be preceded by a `// SAFETY:` comment. This comment must explain why the code is safe (what conditions or guarantees make it so) and, where possible, link to supporting documentation.

```rust
// ✅ Permitted — FFI usage with a documented safety justification
// SAFETY: `ptr` is guaranteed non-null and valid for `len` bytes
//   by the C caller contract in ffi_contract.md §3.2.
unsafe {
  std::slice::from_raw_parts(ptr, len)
}

// ❌ Forbidden — no SAFETY comment, no explanation
unsafe {
  *raw_ptr = 42;
}
```

---

### 4. Keep all `use` statements at the top of the file — never use fully-qualified paths inline

**Where imports go:** Every `use` statement must be declared in the file's header — not inside functions, `impl` blocks, or `match` arms. This lets anyone see all of a file's dependencies at a glance.

**How to reference imports:** Never write a fully-qualified path inline (e.g., `std::collections::HashMap` used directly in code). Instead, import the item with a `use` statement at the top of the file, then reference it by its short name (e.g., `HashMap`).

This applies to everything — standard library items, external crates, and internal project paths alike (including `crate::foo::Bar` or `super::foo::Bar`).

```rust
// ✅ Correct — all imports declared at the top; no fully-qualified paths in the code
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::foo::Bar;

pub fn build_index(items: &[&str]) -> HashMap<&str, usize> {
  items.iter().enumerate().map(|(i, k)| (*k, i)).collect()
}

pub fn handle_state(state: &Bar) {
  // ...
}
```

```rust
// ❌ Forbidden — `use` statement hidden inside the function body
pub fn build_index(items: &[&str]) -> std::collections::HashMap<&str, usize> {
  use std::collections::HashMap; // hidden dependency, easy to miss
  items.iter().enumerate().map(|(i, k)| (*k, i)).collect()
}

// ❌ Forbidden — fully-qualified path used instead of a top-level import
pub fn build_index_fq(items: &[&str]) -> std::collections::HashMap<&str, usize> {
  let map = std::collections::HashMap::new();
  // ...
  map
}

// ❌ Forbidden — internal crate path used inline instead of imported
pub fn process_state(state: &crate::foo::Bar) {
  // ...
}
```
