# Kani Development Guide

## Commands

Always run project commands through `make`. Do **not** call `cargo` directly — see the "Restrictions" section below for why this matters.

```cmd
make check                     # Compiles the code and runs the linter only — fast, does not execute tests
make test                      # Runs the entire test suite (unit tests + integration tests) — slow
make test -- -p package_name   # Runs all tests for one specific package — slow
make test my_test_case         # Runs a single test, test module, or test group — fast
make bench                     # Runs all performance benchmarks — very slow, use only when necessary
```

---

## Restrictions

### 1. Never edit the `[profile.*]` sections in `Cargo.toml`

The `[profile.*]` tables (for example, `[profile.release]` or `[profile.dev]`) control compiler optimization settings and are locked. Changing them affects the entire project's build behavior and can quietly break reproducibility (i.e., builds may no longer produce identical results across machines or over time).

```toml
# ✅ Allowed — adding a new dependency
[dependencies]
serde = { version = "1", features = ["derive"] }

# ❌ Forbidden — modifying any [profile.*] block
[profile.release]
opt-level = 3
```

---

### 2. Never run `cargo test` directly

`make test` includes safety protections — such as timeouts — that stop the test run if something hangs or misbehaves. Running `cargo test` directly skips these protections entirely. Always use the `make test` commands shown above instead.

```sh
# ✅ Correct
make test
make test -- -p package_name
make test my_test_case

# ❌ Forbidden — no timeout or safety protection
cargo test
cargo test -p package_name
cargo test my_test_case
```

---

### 3. Avoid `unsafe` code blocks

`unsafe` blocks in Rust bypass the compiler's normal safety checks, so they should be used only in two situations:

- **FFI (Foreign Function Interface)**: Code that interacts with non-Rust code (such as C libraries).
- **Performance-critical code**: Only after profiling has proven that the `unsafe` block provides a measurable speed benefit.

Every `unsafe` block **must** include a `// SAFETY:` comment directly above it. This comment must explain exactly why the code is safe (what conditions or guarantees make it safe) and point to supporting documentation (an "audit trail").

```rust
// ✅ Permitted — FFI usage with a documented safety justification
// SAFETY: `ptr` is guaranteed non-null and valid for `len` bytes
//   by the C caller contract in ffi_contract.md §3.2.
unsafe {
  std::slice::from_raw_parts(ptr, len)
}

// ❌ Forbidden — no explanation, no SAFETY comment
unsafe {
  *raw_ptr = 42;
}
```

---

### 4. Place all `use` (import) statements at the top of each file

Every `use` statement should appear in the file's header section — not inside functions, `impl` blocks, or `match` arms. Keeping imports at the top makes all of a file's dependencies visible at a glance and avoids the need to write out long, fully-qualified paths (e.g., `std::collections::HashMap`) elsewhere in the code.

```rust
// ✅ Correct — all imports declared at the top of the file
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub fn build_index(items: &[&str]) -> HashMap<&str, usize> {
  items.iter().enumerate().map(|(i, k)| (*k, i)).collect()
}

// ❌ Forbidden — import hidden inside a function body
pub fn build_index(items: &[&str]) -> std::collections::HashMap<&str, usize> {
  use std::collections::HashMap;  // hidden dependency, hard to spot
  items.iter().enumerate().map(|(i, k)| (*k, i)).collect()
}
```
