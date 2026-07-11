# Kani Development Guide

## Commands

Always run project commands through `make`. Do **not** call `cargo` directly — see "Restrictions" below for why.

```cmd
make check                     # Compiles and runs the linter on all project — slow
make check -- -p package_name  # Compiles and runs the linter on one specific package — fast
make test                      # Runs all tests in all project — very slow
make test -- -p package_name   # Runs all tests in one specific package — slow
make test my_test_case         # Runs a single test, test module, or test group — fast
make bench                     # Runs all performance benchmarks — very slow, use sparingly
```

---

## Restrictions

### 1. Never edit the `[profile.*]` sections in `Cargo.toml`

The `[profile.*]` tables (e.g., `[profile.release]`, `[profile.dev]`) control compiler optimization settings and are locked. Changing them affects the build behavior of the entire project and can silently break reproducibility — meaning the same code might no longer produce identical builds across different machines or at different times.

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

`make test` adds safety protections — such as timeouts — that automatically stop a test run if something hangs or misbehaves. Running `cargo test` directly skips these protections. Always use one of the `make test` commands shown above instead.

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

`unsafe` blocks turn off Rust's normal compile-time safety checks. Use them only in these two cases:

- **FFI (Foreign Function Interface):** code that interacts with non-Rust code, such as a C library.
- **Performance-critical code:** only after profiling has proven that the `unsafe` block gives a measurable speed improvement.

Every `unsafe` block **must** have a `// SAFETY:` comment directly above it. This comment must explain exactly why the code is safe (what conditions or guarantees make it safe) and reference supporting documentation as an audit trail.

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

### 4. Put all `use` (import) statements at the top of the file, and never use fully-qualified paths

Every `use` statement must appear in the file's header section — not inside functions, `impl` blocks, or `match` arms. Keeping imports at the top lets anyone see all of a file's dependencies at a glance.

In addition, fully-qualified paths (e.g., writing `std::collections::HashMap` inline instead of importing `HashMap`) are not allowed anywhere in the code. Every external type, module, macro, or standard library item must be brought in with a `use` statement at the top of the file, then referenced by its short name.

```rust
// ✅ Correct — all imports declared at the top; no fully-qualified paths in the code
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub fn build_index(items: &[&str]) -> HashMap<&str, usize> {
  items.iter().enumerate().map(|(i, k)| (*k, i)).collect()
}

// ❌ Forbidden — `use` statement hidden inside the function body
pub fn build_index(items: &[&str]) -> std::collections::HashMap<&str, usize> {
  use std::collections::HashMap;  // hidden dependency, easy to miss
  items.iter().enumerate().map(|(i, k)| (*k, i)).collect()
}

// ❌ Forbidden — fully-qualified path used instead of a top-level import
pub fn build_index_fq(items: &[&str]) -> std::collections::HashMap<&str, usize> {
  let map = std::collections::HashMap::new();
  // ...
  map
}
```
