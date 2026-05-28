# Workspace Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set up the Cargo workspace, toolchain, and empty crate skeletons so subsequent plans can fill in each crate.

**Architecture:** A Cargo workspace at the repo root with 5 member crates under `crates/`: `core`, `sql`, `schema`, `http`, `bin`. Dependency rule: each crate only depends on those earlier in the list.

**Tech Stack:** Rust (stable, pinned via `rust-toolchain.toml`), Cargo workspaces.

---

### Task 1: Toolchain pin + gitignore

**Files:**
- Create: `rust-toolchain.toml`
- Create: `.gitignore`

- [ ] **Step 1: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.82.0"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

- [ ] **Step 2: Write `.gitignore`**

```gitignore
/target
**/*.rs.bk
Cargo.lock.bak
.env
.DS_Store
```

(Keep `Cargo.lock` tracked — this is a binary workspace.)

- [ ] **Step 3: Commit**

```bash
git add rust-toolchain.toml .gitignore
git commit -m "chore: pin Rust toolchain and add gitignore"
```

---

### Task 2: Workspace root manifest

**Files:**
- Create: `Cargo.toml`

- [ ] **Step 1: Write workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
    "crates/core",
    "crates/sql",
    "crates/schema",
    "crates/http",
    "crates/bin",
]

[workspace.package]
edition = "2021"
rust-version = "1.82"
license = "MIT OR Apache-2.0"
authors = ["rustapi contributors"]

[workspace.dependencies]
# Async
tokio = { version = "1.40", features = ["macros", "rt-multi-thread", "signal", "sync"] }

# Web
axum = { version = "0.7", features = ["macros"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace"] }
hyper = "1.4"

# Database
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "macros", "migrate", "uuid", "chrono", "json"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Errors
thiserror = "1"
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }

# Time / IDs
chrono = { version = "0.4", default-features = false, features = ["serde", "clock"] }
uuid = { version = "1", features = ["serde", "v4"] }

# Tests
tokio-test = "0.4"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
```

- [ ] **Step 2: Verify it parses (no crates yet — will fail on missing members)**

Run: `cargo metadata --no-deps --format-version 1 2>&1 | head -5`
Expected: error about missing `crates/core/Cargo.toml`. That's fine — we create them next.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: scaffold Cargo workspace with shared dependencies"
```

---

### Task 3: Create empty crate skeletons

**Files:**
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/sql/Cargo.toml`
- Create: `crates/sql/src/lib.rs`
- Create: `crates/schema/Cargo.toml`
- Create: `crates/schema/src/lib.rs`
- Create: `crates/http/Cargo.toml`
- Create: `crates/http/src/lib.rs`
- Create: `crates/bin/Cargo.toml`
- Create: `crates/bin/src/main.rs`

- [ ] **Step 1: `crates/core/Cargo.toml`**

```toml
[package]
name = "rustapi-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
chrono.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: `crates/core/src/lib.rs`**

```rust
#![forbid(unsafe_code)]
```

- [ ] **Step 3: `crates/sql/Cargo.toml`**

```toml
[package]
name = "rustapi-sql"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
rustapi-core = { path = "../core" }
chrono.workspace = true
uuid.workspace = true
serde_json.workspace = true
```

Note: deliberately NO `sqlx` here — the spec requires this crate stay DB-driver-free so it's unit-testable.

- [ ] **Step 4: `crates/sql/src/lib.rs`**

```rust
#![forbid(unsafe_code)]
```

- [ ] **Step 5: `crates/schema/Cargo.toml`**

```toml
[package]
name = "rustapi-schema"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
rustapi-core = { path = "../core" }
rustapi-sql = { path = "../sql" }
sqlx.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
chrono.workspace = true
uuid.workspace = true
```

- [ ] **Step 6: `crates/schema/src/lib.rs`**

```rust
#![forbid(unsafe_code)]
```

- [ ] **Step 7: `crates/http/Cargo.toml`**

```toml
[package]
name = "rustapi-http"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
rustapi-core = { path = "../core" }
rustapi-sql = { path = "../sql" }
rustapi-schema = { path = "../schema" }
axum.workspace = true
tower.workspace = true
tower-http.workspace = true
tokio.workspace = true
sqlx.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
chrono.workspace = true
uuid.workspace = true
async-trait = "0.1"
```

- [ ] **Step 8: `crates/http/src/lib.rs`**

```rust
#![forbid(unsafe_code)]
```

- [ ] **Step 9: `crates/bin/Cargo.toml`**

```toml
[package]
name = "rustapi"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "rustapi"
path = "src/main.rs"

[dependencies]
rustapi-core = { path = "../core" }
rustapi-schema = { path = "../schema" }
rustapi-http = { path = "../http" }
tokio.workspace = true
sqlx.workspace = true
axum.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
anyhow.workspace = true

[dev-dependencies]
reqwest.workspace = true
serde_json.workspace = true
uuid.workspace = true
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres"] }
tokio.workspace = true
```

- [ ] **Step 10: `crates/bin/src/main.rs`**

```rust
fn main() {
    println!("rustapi bootstrap");
}
```

- [ ] **Step 11: Build the whole workspace**

Run: `cargo build --workspace`
Expected: PASS (slow first build — downloads deps).

- [ ] **Step 12: Commit**

```bash
git add crates Cargo.lock
git commit -m "chore: scaffold workspace member crates"
```

---

### Task 4: README placeholder + lint config

**Files:**
- Create: `README.md`
- Create: `.cargo/config.toml`

- [ ] **Step 1: Minimal `README.md`**

```markdown
# rustapi

Headless CMS framework in Rust. v1 in progress.

See [design spec](docs/superpowers/specs/2026-05-28-rustapi-core-design.md).

## Dev

```sh
cargo build --workspace
cargo test --workspace
```
```

- [ ] **Step 2: `.cargo/config.toml`** — turn key clippy warns into errors in CI-style builds (does not block local dev)

```toml
[build]
# Keep default

[target.'cfg(all())']
rustflags = [
    "-Dwarnings",
]
```

- [ ] **Step 3: Verify build still passes**

Run: `cargo build --workspace`
Expected: PASS, zero warnings.

- [ ] **Step 4: Commit**

```bash
git add README.md .cargo/config.toml
git commit -m "chore: add README and warning-as-error cargo config"
```

---

## Self-Review Notes

- Spec §2.1 crate layout: covered by Task 3.
- Spec §2.2 extensibility seams: implemented per-crate in plans 01–04.
- Toolchain pinned per Task 1 — keeps CI reproducible.
- `sqlx` deliberately absent from `rustapi-sql` per spec rule.
- No business logic in this plan — purely structural setup.
