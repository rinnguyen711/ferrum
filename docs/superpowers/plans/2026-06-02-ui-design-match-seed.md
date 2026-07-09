# UI Design-Match + Default Types & Seed — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a fresh install ship with default Article/Author/Category types + seed data, and bring the admin UI up to the reference design in `design/` with visible placeholders for unimplemented features.

**Architecture:** Two phases. **Phase A (backend):** a `seed` module in the `ferrum` bin crate runs at boot; if the DB has no content types and `FERRUM_SEED` is on, it creates the three types via `SchemaService::create` and inserts rows via the http crate's `body_to_binds` + `ferrum_sql::insert`. **Phase B (UI):** upgrade React screens (Dashboard, ContentList, SchemaEditor, shell panels, Settings) to match the design and add a placeholder MediaLibrary screen. Spec: `docs/superpowers/specs/2026-06-02-ui-design-match-and-seed-design.md`.

**Tech Stack:** Rust (axum, sqlx, anyhow, tracing, testcontainers), React + TypeScript + Vite, existing `rs-*` CSS in `ui/src/styles.css` + reference CSS in `design/ferrum/styles.css`.

**Relation ordering note (critical):** `SchemaService::create` calls `validate_relation_cross_refs`, which requires a relation's `target` type to already exist. Create order is **author → category → article**. The `article.author` relation declares `inverse: "articles"`, which registers the Author↔Article inverse automatically (same pattern as `post.author` with `inverse: "posts"` in `crates/bin/tests/relations.rs`). Do **not** put an `articles` relation field on the author type — it would reference `article` before it exists and fail.

**Field-kind mapping (design → API):** design `Text`→`text`, `UID`/slug→`slug`, `Enumeration`→`enum`, `Boolean`→`boolean`, `Number`/integer→`integer`, `Datetime`→`datetime`, `Relation`→`relation`. The design's `Media`/`Rich text` kinds have no API equivalent: `body`→`text`, and `cover`/`avatar` media fields are **omitted** from seed schemas.

---

## Phase A — Backend Seed

### Task A1: Add `seed` config flag

**Files:**
- Modify: `crates/bin/src/config.rs`

- [ ] **Step 1: Add `seed` field + env parse**

In `crates/bin/src/config.rs`, add `pub seed: bool` to the `Config` struct (after `studio_dir`):

```rust
    pub studio_dir: Option<String>,
    /// When true (default), an empty DB is seeded with default content types
    /// and sample data at startup. Set FERRUM_SEED=false to disable.
    pub seed: bool,
```

In `from_env`, before the final `Ok(Self { ... })`, add:

```rust
        let seed = std::env::var("FERRUM_SEED")
            .ok()
            .map(|s| !matches!(s.as_str(), "0" | "false" | "no"))
            .unwrap_or(true);
```

And add `seed,` to the returned struct literal.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p ferrum`
Expected: builds clean (the existing `rejects_short_key` test is unaffected).

- [ ] **Step 3: Commit**

```bash
git add crates/bin/src/config.rs
git commit -m "feat(bin): add FERRUM_SEED config flag (default on)"
```

---

### Task A2: Seed module — content-type definitions

**Files:**
- Create: `crates/bin/src/seed.rs`
- Modify: `crates/bin/Cargo.toml` (add `serde_json` to `[dependencies]`)
- Modify: `crates/bin/src/main.rs` (add `mod seed;`)

This task only builds the three `NewContentType` values and a `seed_types` helper that creates them in order. Entry insertion comes in A3.

- [ ] **Step 1: Add `serde_json` to bin dependencies**

In `crates/bin/Cargo.toml`, under `[dependencies]` (it is currently only a dev-dependency), add:

```toml
serde_json.workspace = true
```

- [ ] **Step 2: Create `crates/bin/src/seed.rs` with type builders**

```rust
//! First-boot seeding: default Article/Author/Category types + sample data.
//! Idempotent — skips entirely if any content type already exists.

use anyhow::Result;
use ferrum_core::{Field, FieldKind, NewContentType};
use ferrum_schema::SchemaService;
use serde_json::json;

fn field(name: &str, kind: FieldKind, required: bool) -> Field {
    Field {
        name: name.into(),
        kind,
        required,
        unique: false,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta: json!({}),
    }
}

fn enum_field(name: &str, values: &[&str], required: bool) -> Field {
    Field {
        kind_meta: json!({ "values": values }),
        ..field(name, FieldKind::Enum, required)
    }
}

/// many_to_one relation `name` -> `target`, optionally registering an inverse
/// field `inverse` on the target type.
fn relation_field(name: &str, target: &str, inverse: Option<&str>, required: bool) -> Field {
    let mut meta = json!({ "target": target, "cardinality": "many_to_one" });
    if let Some(inv) = inverse {
        meta["inverse"] = json!(inv);
    }
    Field {
        kind_meta: meta,
        ..field(name, FieldKind::Relation, required)
    }
}

fn author_type() -> NewContentType {
    NewContentType {
        name: "author".into(),
        display_name: "Author".into(),
        fields: vec![
            field("name", FieldKind::String, true),
            field("role", FieldKind::String, false),
            field("bio", FieldKind::Text, false),
        ],
    }
}

fn category_type() -> NewContentType {
    NewContentType {
        name: "category".into(),
        display_name: "Category".into(),
        fields: vec![
            field("name", FieldKind::String, true),
            field("slug", FieldKind::Slug, true),
            field("color", FieldKind::String, false),
            field("description", FieldKind::Text, false),
        ],
    }
}

fn article_type() -> NewContentType {
    NewContentType {
        name: "article".into(),
        display_name: "Article".into(),
        fields: vec![
            field("title", FieldKind::String, true),
            field("slug", FieldKind::Slug, true),
            enum_field("status", &["draft", "review", "published"], true),
            field("excerpt", FieldKind::Text, false),
            field("body", FieldKind::Text, false),
            // inverse "articles" registers the Author<->Article back-reference.
            relation_field("author", "author", Some("articles"), false),
            field("featured", FieldKind::Boolean, false),
            field("read_time", FieldKind::Integer, false),
            field("published_at", FieldKind::Datetime, false),
        ],
    }
}

/// Create the three default types in dependency order. Returns Ok(true) if
/// types were created, Ok(false) if the DB already had content types.
pub async fn seed_types(schemas: &SchemaService) -> Result<bool> {
    if !schemas.registry().list().await.is_empty() {
        return Ok(false);
    }
    for ct in [author_type(), category_type(), article_type()] {
        schemas
            .create(ct)
            .await
            .map_err(|e| anyhow::anyhow!("seed create type failed: {e}"))?;
    }
    Ok(true)
}
```

Note: `article` deliberately has no `categories` field. A single many_to_one
relation per the design's category link would point each article at one
category; the design's many-categories is many_to_many, which v2.4 does not
support (`cardinality must be "many_to_one"`). Seed keeps `author` only;
categories are seeded as their own browsable collection. (Revisit if
many_to_many lands.)

- [ ] **Step 3: Wire `mod seed;` into main**

In `crates/bin/src/main.rs`, add after `mod config;`:

```rust
mod seed;
```

- [ ] **Step 4: Verify compile**

Run: `cargo build -p ferrum`
Expected: builds clean.

- [ ] **Step 5: Commit**

```bash
git add crates/bin/Cargo.toml crates/bin/src/seed.rs crates/bin/src/main.rs
git commit -m "feat(bin): seed module — default content-type definitions"
```

---

### Task A3: Seed module — sample data rows

**Files:**
- Modify: `crates/bin/src/seed.rs`

Insert authors + categories + articles by reusing the http write path. `body_to_binds` is `pub` in `ferrum_http::entry`; `ferrum_sql::insert` and `ferrum_schema::bind::bind_all` are `pub`.

- [ ] **Step 1: Add imports + an `insert_entry` helper**

At the top of `crates/bin/src/seed.rs`, extend imports:

```rust
use ferrum_core::ContentType;
use ferrum_http::entry::body_to_binds;
use ferrum_schema::bind::bind_all;
use serde_json::{Map, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;
```

Add this helper (entries skip relation existence checks — seed controls the
ids it passes, so the FK is guaranteed to exist):

```rust
/// Insert one row for `ct` from a JSON object, returning the new row id.
async fn insert_entry(pool: &PgPool, ct: &ContentType, body: Map<String, Value>) -> Result<Uuid> {
    let (binds, _checks) = body_to_binds(ct, body, true)
        .map_err(|e| anyhow::anyhow!("seed body_to_binds: {e}"))?;
    let (sql, bind_vals) = ferrum_sql::insert(ct, &binds)
        .map_err(|e| anyhow::anyhow!("seed insert sql: {e}"))?;
    let row = bind_all(sqlx::query(&sql), &bind_vals)
        .fetch_one(pool)
        .await?;
    let id: Uuid = row.try_get("id")?;
    Ok(id)
}
```

- [ ] **Step 2: Add the data + `seed_rows` function**

Append to `crates/bin/src/seed.rs`:

```rust
/// Insert sample authors, categories, and articles (articles link a real
/// author). Assumes the three types were just created.
pub async fn seed_rows(pool: &PgPool, schemas: &SchemaService) -> Result<()> {
    let author_ct = schemas.registry().get("author").await
        .ok_or_else(|| anyhow::anyhow!("author type missing during seed"))?;
    let category_ct = schemas.registry().get("category").await
        .ok_or_else(|| anyhow::anyhow!("category type missing during seed"))?;
    let article_ct = schemas.registry().get("article").await
        .ok_or_else(|| anyhow::anyhow!("article type missing during seed"))?;

    // --- authors --- (name, role, bio) -> capture id by name
    let authors = [
        ("Mara Velez", "Editor in chief", "Runs the desk. Twelve years in long-form science journalism."),
        ("Idris Bello", "Staff writer", "Covers climate, energy, and the people in between."),
        ("Saoirse Lynch", "Contributor", "Essayist. Writes about cities, memory, and maps."),
        ("Tomas Reier", "Photo editor", "Pictures first, words later."),
    ];
    let mut author_id = std::collections::HashMap::new();
    for (name, role, bio) in authors {
        let mut b = Map::new();
        b.insert("name".into(), json!(name));
        b.insert("role".into(), json!(role));
        b.insert("bio".into(), json!(bio));
        let id = insert_entry(pool, &author_ct, b).await?;
        author_id.insert(name, id);
    }

    // --- categories --- (name, slug, color, description)
    let categories = [
        ("Science", "science", "#0E7490", "Research, discovery, and the scientific method."),
        ("Climate", "climate", "#15803D", "Energy, environment, and a changing planet."),
        ("Culture", "culture", "#7C3AED", "Arts, ideas, and how we live."),
        ("Cities", "cities", "#C2410C", "Urban life and the built environment."),
        ("Interviews", "interviews", "#B45309", "Long-form conversations."),
    ];
    for (name, slug, color, description) in categories {
        let mut b = Map::new();
        b.insert("name".into(), json!(name));
        b.insert("slug".into(), json!(slug));
        b.insert("color".into(), json!(color));
        b.insert("description".into(), json!(description));
        insert_entry(pool, &category_ct, b).await?;
    }

    // --- articles --- (title, slug, status, excerpt, author-name, featured, read_time, published_at)
    let articles = [
        ("The quiet reinvention of the tidal turbine", "tidal-turbine-reinvention", "published", "A new generation of low-speed rotors is making estuary power viable for the first time.", "Idris Bello", true, 9, Some("2026-05-28T09:00:00Z")),
        ("What a city remembers when its river is gone", "city-remembers-river", "published", "Walking the buried waterways of four cities that paved over their founding streams.", "Saoirse Lynch", false, 14, Some("2026-05-27T07:30:00Z")),
        ("The lab growing coral in the dark", "coral-in-the-dark", "draft", "Inside a basement aquarium where bleaching has been reversed — for now.", "Idris Bello", false, 7, None),
        ("Forty years of the same weather diary", "weather-diary-forty-years", "published", "A retired postmaster recorded the sky every morning. The data turned out to matter.", "Mara Velez", false, 11, Some("2026-05-24T06:00:00Z")),
        ("The mapmakers who refuse to draw borders", "mapmakers-no-borders", "review", "A small cartography collective is redrawing the world without nation-states.", "Saoirse Lynch", false, 8, None),
        ("Why your bread tastes different at altitude", "bread-at-altitude", "published", "Pressure, yeast, and the chemistry of a mountain-town bakery.", "Mara Velez", false, 5, Some("2026-05-21T08:00:00Z")),
        ("An interview with the last lighthouse keeper", "last-lighthouse-keeper", "draft", "Forty-one years on a rock in the North Atlantic, in his own words.", "Mara Velez", false, 16, None),
        ("The return of the night train", "return-of-night-train", "published", "Europe rebuilt its sleeper network. We rode it for a week to see if it works.", "Idris Bello", true, 10, Some("2026-05-19T07:00:00Z")),
        ("A field guide to urban lichen", "urban-lichen-field-guide", "review", "The pollution map hiding in plain sight on every old stone wall.", "Tomas Reier", false, 6, None),
        ("The economics of a free public sauna", "free-public-sauna", "published", "One northern city bet that warmth should be a commons. The numbers are surprising.", "Saoirse Lynch", false, 12, Some("2026-05-17T08:30:00Z")),
    ];
    for (title, slug, status, excerpt, author_name, featured, read_time, published_at) in articles {
        let mut b = Map::new();
        b.insert("title".into(), json!(title));
        b.insert("slug".into(), json!(slug));
        b.insert("status".into(), json!(status));
        b.insert("excerpt".into(), json!(excerpt));
        b.insert("featured".into(), json!(featured));
        b.insert("read_time".into(), json!(read_time));
        if let Some(pa) = published_at {
            b.insert("published_at".into(), json!(pa));
        }
        if let Some(aid) = author_id.get(author_name) {
            b.insert("author".into(), json!(aid.to_string()));
        }
        insert_entry(pool, &article_ct, b).await?;
    }

    Ok(())
}

/// Top-level entry point: seed types + rows when the DB is empty and seeding
/// is enabled. Non-fatal on row errors — logs and continues so the server boots.
pub async fn seed_if_empty(pool: &PgPool, schemas: &SchemaService, enabled: bool) -> Result<()> {
    if !enabled {
        return Ok(());
    }
    match seed_types(schemas).await {
        Ok(false) => {
            tracing::debug!("seed: content types already present, skipping");
            return Ok(());
        }
        Ok(true) => {}
        Err(e) => {
            tracing::warn!(error = %e, "seed: type creation failed, skipping data seed");
            return Ok(());
        }
    }
    if let Err(e) = seed_rows(pool, schemas).await {
        tracing::warn!(error = %e, "seed: sample data insert failed (types still created)");
        return Ok(());
    }
    tracing::info!("seed: created default types (author, category, article) + sample data");
    Ok(())
}
```

- [ ] **Step 3: Verify compile**

Run: `cargo build -p ferrum`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/src/seed.rs
git commit -m "feat(bin): seed sample authors, categories, articles with relations"
```

---

### Task A4: Wire seed into startup

**Files:**
- Modify: `crates/bin/src/main.rs`

- [ ] **Step 1: Call `seed_if_empty` after the registry is hydrated**

In `crates/bin/src/main.rs`, after the line:

```rust
    let schemas = SchemaService::new(pool.clone(), registry.clone());
```

add:

```rust
    seed::seed_if_empty(&pool, &schemas, cfg.seed)
        .await
        .context("seed default content")?;
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p ferrum`
Expected: builds clean.

- [ ] **Step 3: Commit**

```bash
git add crates/bin/src/main.rs
git commit -m "feat(bin): run seed_if_empty at startup"
```

---

### Task A5: Integration test for seeding

**Files:**
- Create: `crates/bin/tests/seed.rs`

Reuses the existing `common` harness, but the harness builds its own router
without seeding, so the test calls the seed functions directly against the
harness's pool + schemas. Since `TestApp` doesn't expose `schemas`, the test
constructs its own `SchemaService` from the harness pool (cheap; the registry
hydrates from the same DB).

- [ ] **Step 1: Write the test**

Create `crates/bin/tests/seed.rs`:

```rust
//! Seeding integration test. Boots a real Postgres via testcontainers, runs
//! the seed functions against a fresh DB, and asserts the default types and
//! sample rows exist with working relations. Also asserts idempotency.

mod common;
use common::TestApp;
use serde_json::Value;

#[tokio::test]
async fn seeds_default_types_and_data() {
    let app = TestApp::spawn().await;

    // Build a SchemaService over the harness pool (fresh DB → empty registry).
    use ferrum_schema::{SchemaRegistry, SchemaService};
    let registry = SchemaRegistry::new();
    registry.reload_from_db(&app.pool).await.unwrap();
    let schemas = SchemaService::new(app.pool.clone(), registry);

    // First run seeds; returns Ok and creates types.
    ferrum::seed::seed_if_empty(&app.pool, &schemas, true)
        .await
        .unwrap();

    // 3 content types exist.
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let types: Value = resp.json().await.unwrap();
    let names: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"author"), "author type: {names:?}");
    assert!(names.contains(&"category"), "category type: {names:?}");
    assert!(names.contains(&"article"), "article type: {names:?}");

    // Row counts.
    let count = |path: &str| {
        let app = &app;
        async move {
            let r = app.admin(app.client.get(app.url(path))).send().await.unwrap();
            assert_eq!(r.status(), 200, "{}", path);
            let v: Value = r.json().await.unwrap();
            v["meta"]["total"].as_i64().unwrap()
        }
    };
    assert_eq!(count("/api/author").await, 4);
    assert_eq!(count("/api/category").await, 5);
    assert_eq!(count("/api/article").await, 10);

    // A populated article resolves its author to a real object.
    let r = app
        .admin(app.client.get(app.url("/api/article?populate=author&pageSize=1")))
        .send()
        .await
        .unwrap();
    let v: Value = r.json().await.unwrap();
    let first = &v["data"][0];
    assert!(
        first["author"]["name"].is_string(),
        "author should populate: {first}"
    );

    // Idempotent: second run is a no-op (returns without error, no duplicates).
    ferrum::seed::seed_if_empty(&app.pool, &schemas, true)
        .await
        .unwrap();
    assert_eq!(count("/api/author").await, 4, "no duplicate authors");
}
```

- [ ] **Step 2: Make the bin's modules reachable from the integration test**

Integration tests can only import the crate's **library** target, but `ferrum`
is a bin-only crate. Add a minimal lib target exposing `seed`.

Create `crates/bin/src/lib.rs`:

```rust
//! Library facet of the ferrum binary, exposing modules that integration
//! tests need to call directly (e.g. seeding).
pub mod config;
pub mod seed;
```

In `crates/bin/Cargo.toml`, add a `[lib]` section above `[[bin]]`:

```toml
[lib]
name = "ferrum"
path = "src/lib.rs"
```

In `crates/bin/src/main.rs`, replace the `mod config;` / `mod seed;` lines with:

```rust
use ferrum::{config, seed};
```

(Keep `use config::Config;` working — change it to `use crate::config::Config;`
→ actually `use config::Config;` still resolves via the `use ferrum::config`
import; if the compiler objects, write `use ferrum::config::Config;` and drop
the separate `use config::Config;` line.)

- [ ] **Step 3: Run the seed test**

Run: `cargo test -p ferrum --test seed -- --nocapture`
Expected: PASS (boots Postgres via testcontainers; needs Docker running).

- [ ] **Step 4: Run the full bin test suite to confirm no regressions**

Run: `cargo test -p ferrum`
Expected: all existing relation/filter/fieldkind tests still PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/bin/src/lib.rs crates/bin/Cargo.toml crates/bin/src/main.rs crates/bin/tests/seed.rs
git commit -m "test(bin): seeding integration test + lib target"
```

---

### Task A6: Document the seed flag

**Files:**
- Modify: `README.md`
- Modify: `docker-compose.yml`

- [ ] **Step 1: Note FERRUM_SEED in README**

In `README.md`, in the Docker section right after the admin-key override block, add:

```markdown
The demo seeds default **Article**, **Author**, and **Category** types with
sample data on first boot (empty DB only). Disable with `FERRUM_SEED=false`.
```

- [ ] **Step 2: Document in docker-compose**

In `docker-compose.yml`, in the app service `environment:` block, add a commented line:

```yaml
      # FERRUM_SEED: "false"   # disable first-boot sample data
```

(Place it alongside the existing env vars; match the file's existing indentation.)

- [ ] **Step 3: Commit**

```bash
git add README.md docker-compose.yml
git commit -m "docs: document FERRUM_SEED first-boot seeding"
```

---

## Phase B — UI Design-Match

For all Phase B tasks, after edits run `pnpm -C ui build` (typecheck + bundle)
and expect a clean build. CSS classes referenced below that are missing from
`ui/src/styles.css` are ported from `design/ferrum/styles.css` in Task B1.

### Task B1: Port missing CSS

**Files:**
- Modify: `ui/src/styles.css`
- Reference: `design/ferrum/styles.css`

- [ ] **Step 1: Identify the missing classes**

Run:

```bash
grep -oh "rs-[a-z-]*" design/ferrum/styles.css | sort -u > /tmp/d.txt
grep -oh "rs-[a-z-]*" ui/src/styles.css | sort -u > /tmp/u.txt
comm -23 /tmp/d.txt /tmp/u.txt
```

Expected list includes the dashboard/builder/media/settings/relation classes
used by later tasks: `rs-dash`, `rs-dash-hero`, `rs-dash-eyebrow`, `rs-dash-sub`,
`rs-stat-grid`, `rs-stat`, `rs-stat-icon`, `rs-stat-body`, `rs-stat-label`,
`rs-stat-value`, `rs-stat-delta`, `rs-dash-cols`, `rs-dash-card`, `rs-dash-card-head`,
`rs-dash-list`, `rs-dash-row`, `rs-dash-row-title`, `rs-sys`, `rs-sys-row`,
`rs-sys-status`, `rs-sys-meta`, `rs-sys-val`, `rs-spark`, `rs-spark-head`,
`rs-spark-svg`, `rs-cm-tabs`, `rs-tab`, `rs-tab-count`, `rs-cm-toolbar`,
`rs-search`, `rs-spacer`, `rs-bulkbar`, `rs-bulkbar-actions`, `rs-col-check`,
`rs-check`, `rs-chips`, `rs-chip`, `rs-cell-author`, `rs-feat`, `rs-pager-ctrl`,
`rs-page-btn`, `rs-select-sm`, `rs-media-grid`, `rs-media-card`, `rs-media-cover`,
`rs-media-ext`, `rs-media-card-meta`, `rs-builder-empty`, `rs-builder-empty-icon`,
`rs-schema`, `rs-schema-head`, `rs-schema-row`, `rs-schema-drag`,
`rs-schema-fieldicon`, `rs-schema-name`, `rs-schema-type`, `rs-schema-actions`,
`rs-schema-add`, `rs-type-pill`, `rs-req-tag`, `rs-swatch`, `rs-empty--lg`,
`rs-preview-pill`, plus any others in the diff.

- [ ] **Step 2: Copy the rule blocks for those classes from `design/ferrum/styles.css` into `ui/src/styles.css`**

Append the matching CSS rule blocks (verbatim from `design/ferrum/styles.css`)
to the end of `ui/src/styles.css`. Include the full rule for each missing
selector and any descendant/`:hover`/`is-*` variants of those selectors that
appear in the design file. Keep the existing `--accent` / theme variables as-is
(do not copy the design's `:root`/`[data-theme]` blocks — the UI already defines
them).

- [ ] **Step 3: Add a placeholder affordance style**

Add at the end of `ui/src/styles.css`:

```css
/* Placeholder affordances for not-yet-implemented features */
.rs-preview-pill {
  display: inline-flex;
  align-items: center;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: .03em;
  text-transform: uppercase;
  padding: 2px 7px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  color: var(--accent);
}
.rs-panel-item:disabled,
.rs-btn[data-placeholder] {
  opacity: .5;
  cursor: not-allowed;
}
```

- [ ] **Step 4: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 5: Commit**

```bash
git add ui/src/styles.css
git commit -m "feat(ui): port design CSS for dashboard, list, builder, media + placeholder styles"
```

---

### Task B2: Generic shell identity + dark-toggle preserved

**Files:**
- Modify: `ui/src/components/shell.tsx`

- [ ] **Step 1: Replace hardcoded "Mara Velez" identity with generic Admin**

In `ui/src/components/shell.tsx`, in the rail foot Avatar (inside `Sidebar`):

```tsx
        <Avatar name="Admin" initials="AD" color="#52525B" size={30} />
```

In the `Topbar` user block, replace the Avatar + meta with:

```tsx
        <div className="rs-topbar-user">
          <Avatar name="Admin" initials="AD" color="#52525B" size={28} />
          <div className="rs-topbar-user-meta">
            <strong>Admin</strong>
            <span>API key</span>
          </div>
          <Icons.chevDown size={15} />
        </div>
```

- [ ] **Step 2: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/shell.tsx
git commit -m "feat(ui): generic Admin identity in rail + topbar"
```

---

### Task B3: Secondary panel — single types & components placeholders + search

**Files:**
- Modify: `ui/src/components/shell.tsx`

- [ ] **Step 1: Add a panel search input + Single types / Components groups**

In `ui/src/components/shell.tsx`, inside `TypePanel`'s `<div className="rs-panel-scroll">`,
add a search box before the existing `PanelGroup`, and after it add disabled
Single types + (builder-only) Components groups:

```tsx
        <div className="rs-panel-search">
          <Icons.search size={15} />
          <input placeholder="Search types" disabled />
        </div>
```

After the existing Collection-types `PanelGroup` closes, before `</div>`:

```tsx
        <div className="rs-panel-group">
          <div className="rs-panel-grouphead"><span>Single types</span></div>
          <button className="rs-panel-item" disabled title="Coming soon">Homepage</button>
          <button className="rs-panel-item" disabled title="Coming soon">Global</button>
        </div>
        {isBuilder && (
          <div className="rs-panel-group">
            <div className="rs-panel-grouphead"><span>Components</span></div>
            <button className="rs-panel-item" disabled title="Coming soon">SEO</button>
            <button className="rs-panel-item" disabled title="Coming soon">Call to action</button>
          </div>
        )}
```

`isBuilder` is already a prop of `TypePanel` — confirm it's in scope (it is).
Ensure `Icons` is imported in this file (it is, at the top).

- [ ] **Step 2: Disable non-API-tokens settings items**

In `SecondaryPanel`'s settings branch, change the items map so only "API tokens"
is enabled:

```tsx
              {g.items.map((it) => {
                const enabled = g.label === "Global settings" && it === "API tokens";
                return (
                  <button
                    key={it}
                    disabled={!enabled}
                    title={enabled ? undefined : "Coming soon"}
                    className={"rs-panel-item" + (enabled ? " is-active" : "")}
                  >
                    {it}
                  </button>
                );
              })}
```

- [ ] **Step 3: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/shell.tsx
git commit -m "feat(ui): panel search + single-type/component/settings placeholders"
```

---

### Task B4: Dashboard redesign

**Files:**
- Modify: `ui/src/screens/Dashboard.tsx`
- Reference: `design/ferrum/screens.jsx` (Dashboard, StatCard, SysRow)

- [ ] **Step 1: Rewrite Dashboard with hero + stat grid + recent + system panel**

Replace the entire body of `ui/src/screens/Dashboard.tsx` with:

```tsx
import { Link } from "react-router-dom";
import { useResource } from "../hooks/useResource";
import { getHealth, listContentTypes, listEntries } from "../api/endpoints";
import { Icons } from "../components/icons";
import { StatusBadge } from "../components/shell";
import { relTime } from "../util";
import type { Entry } from "../api/types";

export function Dashboard() {
  const { data: types, loading, error, refetch } = useResource(
    () => listContentTypes(),
    [],
  );
  const hasArticle = !!types?.some((t) => t.name === "article");
  const articles = useResource(
    () => (hasArticle ? listEntries("article", { pageSize: 100 }) : Promise.resolve(null)),
    [hasArticle],
  );
  const health = useResource(() => getHealth().catch(() => null), []);

  if (loading) return <div className="rs-empty">Loading…</div>;
  if (error)
    return (
      <div className="rs-empty">
        {error.message}{" "}
        <button className="rs-link-btn" onClick={refetch}>Retry</button>
      </div>
    );

  const rows = (articles.data?.data ?? []) as Entry[];
  const byStatus = (s: string) => rows.filter((a) => a["status"] === s).length;
  const recent = [...rows]
    .sort((a, b) => +new Date(b.updated_at) - +new Date(a.updated_at))
    .slice(0, 5);

  return (
    <div className="rs-dash">
      <div className="rs-dash-hero">
        <div>
          <p className="rs-dash-eyebrow rs-mono">Ferrum · workspace</p>
          <h1>Welcome back</h1>
          <p className="rs-dash-sub">
            {types?.length ?? 0} content types registered. The API is{" "}
            {health.data ? "healthy" : "unreachable"}.
          </p>
        </div>
        {hasArticle && (
          <Link to="/content/article/new" className="rs-btn rs-btn--primary rs-btn--lg">
            <Icons.plus size={17} /> New article
          </Link>
        )}
      </div>

      <div className="rs-stat-grid">
        <StatCard label="Published" value={byStatus("published")} delta="live entries" icon="eye" tone="ok" />
        <StatCard label="In review" value={byStatus("review")} delta="needs attention" icon="clock" tone="warn" />
        <StatCard label="Drafts" value={byStatus("draft")} delta="in progress" icon="edit" tone="muted" />
        <StatCard
          label="API"
          value={health.data ? `${health.data.db_ms}ms` : "—"}
          delta={health.data ? `v${health.data.version}` : "offline"}
          icon="bolt"
          tone="accent"
          mono
        />
      </div>

      <div className="rs-dash-cols">
        <section className="rs-dash-card">
          <div className="rs-dash-card-head">
            <h2>Recently edited</h2>
            <Link className="rs-link-btn" to="/content">Open Content Manager →</Link>
          </div>
          <div className="rs-dash-list">
            {recent.length === 0 && <div className="rs-empty">No recent entries.</div>}
            {recent.map((a) => (
              <Link className="rs-dash-row" key={a.id} to={`/content/article/${a.id}`}>
                <span className="rs-dash-row-title">{String(a["title"] ?? a.id)}</span>
                <StatusBadge status={(a["status"] as "draft" | "review" | "published") ?? "draft"} />
                <span className="rs-cell-muted">{relTime(a.updated_at)}</span>
              </Link>
            ))}
          </div>
        </section>

        <section className="rs-dash-card">
          <div className="rs-dash-card-head">
            <h2>System</h2>
            <span className="rs-preview-pill">preview</span>
          </div>
          <div className="rs-sys">
            <SysRow label="API service" value={health.data ? "Healthy" : "Down"} sub="axum · in-process" ok={!!health.data} />
            <SysRow label="Database" value="Healthy" sub="PostgreSQL 16" ok />
            <SysRow label="Build" value={health.data ? `v${health.data.version}` : "—"} sub="cargo" mono />
            <SysRow label="Webhooks" value="0 active" sub="not configured" />
          </div>
          <div className="rs-spark">
            <div className="rs-spark-head"><span>Requests · last hour</span><strong className="rs-mono">—</strong></div>
            <svg viewBox="0 0 240 48" preserveAspectRatio="none" className="rs-spark-svg">
              <polyline
                points="0,40 20,34 40,36 60,28 80,30 100,20 120,24 140,14 160,18 180,10 200,16 220,8 240,12"
                fill="none" stroke="var(--accent)" strokeWidth={2}
              />
            </svg>
          </div>
        </section>
      </div>
    </div>
  );
}

function StatCard({
  label, value, delta, icon, tone, mono,
}: {
  label: string;
  value: string | number;
  delta: string;
  icon: "eye" | "clock" | "edit" | "bolt";
  tone: string;
  mono?: boolean;
}) {
  const I = Icons[icon];
  return (
    <div className={"rs-stat rs-stat--" + tone}>
      <div className="rs-stat-icon"><I size={18} /></div>
      <div className="rs-stat-body">
        <span className="rs-stat-label">{label}</span>
        <strong className={"rs-stat-value" + (mono ? " rs-mono" : "")}>{value}</strong>
        <span className="rs-stat-delta">{delta}</span>
      </div>
    </div>
  );
}

function SysRow({
  label, value, sub, ok, mono,
}: {
  label: string;
  value: string;
  sub: string;
  ok?: boolean;
  mono?: boolean;
}) {
  return (
    <div className="rs-sys-row">
      <span className={"rs-sys-status" + (ok ? " is-ok" : "")} />
      <div className="rs-sys-meta"><strong>{label}</strong><span className="rs-cell-muted">{sub}</span></div>
      <span className={"rs-sys-val" + (mono ? " rs-mono" : "")}>{value}</span>
    </div>
  );
}
```

- [ ] **Step 2: Confirm the icon keys exist**

Run: `grep -oE "eye|clock|edit|bolt|plus" ui/src/components/icons.tsx | sort -u`
Expected: all five appear. If `bolt`/`clock`/`eye`/`edit` is missing, port the
matching path from `design/ferrum/icons.jsx` into `ui/src/components/icons.tsx`
before building (add the key to the `Icons` map with the same SVG path).

- [ ] **Step 3: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ui/src/screens/Dashboard.tsx ui/src/components/icons.tsx
git commit -m "feat(ui): dashboard hero, stat grid, recent + system panel"
```

---

### Task B5: Content list — status tabs, toolbar, rich cells, bulk bar, pager

**Files:**
- Modify: `ui/src/screens/ContentList.tsx`
- Reference: `design/ferrum/content.jsx`

Keep the screen schema-driven (no per-type hardcoding). All new controls are
real where cheap (search, selection, status tabs) and placeholders otherwise
(Filters/Fields/bulk-actions/page-nav), marked with `data-placeholder`.

- [ ] **Step 1: Rewrite ContentList**

Replace the entire body of `ui/src/screens/ContentList.tsx` with:

```tsx
import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar, StatusBadge } from "../components/shell";
import { useResource } from "../hooks/useResource";
import { getContentType, listContentTypes, listEntries } from "../api/endpoints";
import type { ContentType, Entry, Field } from "../api/types";
import { enumValues, relationMeta } from "../api/types";
import { relTime, relationLabel, shortId } from "../util";

const STATUS_TABS: [string, string][] = [
  ["all", "All"],
  ["published", "Published"],
  ["review", "In review"],
  ["draft", "Draft"],
];

export function ContentList() {
  const { type = "" } = useParams<{ type: string }>();
  const navigate = useNavigate();

  const schema = useResource(() => getContentType(type), [type]);
  const allTypes = useResource(() => listContentTypes(), []);

  const ct = schema.data;
  const populate = ct
    ? ct.fields.filter((f) => f.kind === "relation").map((f) => f.name).join(",")
    : "";

  const entries = useResource(
    () => listEntries(type, { populate: populate || undefined, pageSize: 100 }),
    [type, populate],
  );

  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [selected, setSelected] = useState<string[]>([]);

  const hasStatus = !!ct?.fields.some((f) => f.name === "status" && f.kind === "enum");
  const titleField = ct?.fields.find((f) => ["title", "name"].includes(f.name))?.name;

  const rows = entries.data?.data ?? [];
  const filtered = useMemo(() => {
    return rows.filter((e) => {
      if (hasStatus && statusFilter !== "all" && e["status"] !== statusFilter) return false;
      if (query && titleField) {
        const t = String(e[titleField] ?? "").toLowerCase();
        if (!t.includes(query.toLowerCase())) return false;
      }
      return true;
    });
  }, [rows, statusFilter, query, hasStatus, titleField]);

  if (schema.loading || entries.loading) return <div className="rs-empty">Loading…</div>;
  if (schema.error)
    return (
      <div className="rs-empty">
        Couldn’t load type “{type}”.{" "}
        <button className="rs-link-btn" onClick={schema.refetch}>Retry</button>
      </div>
    );
  if (entries.error)
    return (
      <div className="rs-empty">
        {entries.error.message}{" "}
        <button className="rs-link-btn" onClick={entries.refetch}>Retry</button>
      </div>
    );
  if (!ct || !entries.data) return <div className="rs-empty">Unknown content type.</div>;

  const cols = ct.fields;
  const total = entries.data.meta.total;
  const statusCount = (s: string) =>
    s === "all" ? rows.length : rows.filter((e) => e["status"] === s).length;

  const targetSchema = (f: Field): ContentType | undefined => {
    const m = relationMeta(f);
    return m ? allTypes.data?.find((t) => t.name === m.target) : undefined;
  };

  const allOn = filtered.length > 0 && selected.length === filtered.length;
  const toggleAll = () => setSelected(allOn ? [] : filtered.map((e) => e.id));
  const toggle = (id: string) =>
    setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));

  const renderCell = (entry: Entry, f: Field): React.ReactNode => {
    const v = entry[f.name];
    if (v == null || v === "") return <span className="rs-cell-muted">—</span>;
    if (f.name === "status" && f.kind === "enum") {
      return <StatusBadge status={String(v) as "draft" | "review" | "published"} />;
    }
    switch (f.kind) {
      case "relation": {
        const obj = typeof v === "object" ? (v as Entry) : null;
        const label = relationLabel(v, targetSchema(f));
        if (obj && "name" in obj) {
          return (
            <span className="rs-cell-author">
              <Avatar name={String(label)} initials={initials(String(label))} color="#52525B" size={22} />
              {label}
            </span>
          );
        }
        return label;
      }
      case "datetime":
        return relTime(typeof v === "string" ? v : null);
      case "boolean":
        return v ? <Icons.check size={14} /> : <span className="rs-cell-muted">—</span>;
      case "enum":
        return <span className="rs-type-pill">{String(v)}</span>;
      case "json":
        return <code className="rs-mono">{JSON.stringify(v)}</code>;
      default:
        return String(v);
    }
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>{ct.display_name}</h1>
          <p className="rs-cm-sub">{total} entries</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => navigate(`/content/${type}/new`)}>
          <Icons.plus size={16} /> Create new entry
        </button>
      </div>

      {hasStatus && (
        <div className="rs-cm-tabs">
          {STATUS_TABS.map(([k, l]) => (
            <button
              key={k}
              className={"rs-tab" + (statusFilter === k ? " is-active" : "")}
              onClick={() => setStatusFilter(k)}
            >
              {l} <span className="rs-tab-count">{statusCount(k)}</span>
            </button>
          ))}
        </div>
      )}

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input
            placeholder={`Search ${ct.display_name.toLowerCase()}`}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon">
          <Icons.filter size={15} /> Filters
        </button>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon">
          <Icons.layers size={15} /> Fields
        </button>
      </div>

      {selected.length > 0 && (
        <div className="rs-bulkbar">
          <span><strong>{selected.length}</strong> selected</span>
          <div className="rs-bulkbar-actions">
            <button className="rs-btn rs-btn--ghost rs-btn--sm" data-placeholder title="Coming soon">
              <Icons.eye size={14} /> Publish
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm rs-danger" data-placeholder title="Coming soon">
              <Icons.trash size={14} /> Delete
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setSelected([])}>Clear</button>
          </div>
        </div>
      )}

      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th className="rs-col-check">
                <Checkbox checked={allOn} onChange={toggleAll} />
              </th>
              <th className="rs-col-id">ID</th>
              {cols.map((f) => <th key={f.name}>{f.name}</th>)}
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((e) => (
              <tr
                key={e.id}
                className={selected.includes(e.id) ? "is-selected" : ""}
                onClick={() => navigate(`/content/${type}/${e.id}`)}
              >
                <td className="rs-col-check" onClick={(ev) => ev.stopPropagation()}>
                  <Checkbox checked={selected.includes(e.id)} onChange={() => toggle(e.id)} />
                </td>
                <td className="rs-col-id rs-mono">{shortId(e.id)}</td>
                {cols.map((f) => <td key={f.name}>{renderCell(e, f)}</td>)}
                <td className="rs-cell-muted">{relTime(e.updated_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {filtered.length === 0 && <div className="rs-empty">No entries match.</div>}
      </div>

      <div className="rs-pager">
        <span className="rs-cell-muted">Showing {filtered.length} of {total}</span>
        <div className="rs-pager-ctrl">
          <button className="rs-page-btn is-active">1</button>
          <button className="rs-page-btn" data-placeholder disabled title="Coming soon">
            <Icons.chevRight size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}

function Checkbox({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      className={"rs-check" + (checked ? " is-on" : "")}
      onClick={onChange}
      role="checkbox"
      aria-checked={checked}
      type="button"
    >
      {checked && <Icons.check size={13} />}
    </button>
  );
}

function initials(s: string): string {
  return s.split(/\s+/).map((w) => w[0] ?? "").join("").slice(0, 2).toUpperCase() || "?";
}
```

- [ ] **Step 2: Confirm `shortId`, `relationLabel`, `relTime` exist in util**

Run: `grep -oE "export function (shortId|relationLabel|relTime)" ui/src/util.ts | sort -u`
Expected: all three. (They are used by the current ContentList, so they exist.)

- [ ] **Step 3: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add ui/src/screens/ContentList.tsx
git commit -m "feat(ui): content list — status tabs, toolbar, rich cells, bulk bar, pager"
```

---

### Task B6: Media Library placeholder screen

**Files:**
- Create: `ui/src/screens/MediaLibrary.tsx`
- Modify: `ui/src/App.tsx` (route `/media` → MediaLibrary instead of redirect)
- Modify: `ui/src/Layout.tsx` (add `media` to Section + crumbs)
- Modify: `ui/src/components/shell.tsx` (add Media rail item)

- [ ] **Step 1: Create the screen**

Create `ui/src/screens/MediaLibrary.tsx`:

```tsx
import { Icons } from "../components/icons";

const ASSETS = [
  { id: 1, name: "estuary-dawn.jpg", dim: "4096×2731", size: "3.2 MB", hue: 195, ext: "JPG" },
  { id: 2, name: "buried-river-map.png", dim: "2400×3000", size: "1.8 MB", hue: 28, ext: "PNG" },
  { id: 3, name: "coral-tank-01.jpg", dim: "3000×2000", size: "2.6 MB", hue: 270, ext: "JPG" },
  { id: 4, name: "night-train-window.jpg", dim: "3600×2400", size: "4.1 MB", hue: 220, ext: "JPG" },
  { id: 5, name: "lichen-macro.jpg", dim: "2800×2800", size: "2.0 MB", hue: 95, ext: "JPG" },
  { id: 6, name: "sauna-steam.jpg", dim: "3200×2133", size: "2.9 MB", hue: 18, ext: "JPG" },
];

export function MediaLibrary() {
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Media Library <span className="rs-preview-pill">preview</span></h1>
          <p className="rs-cm-sub">{ASSETS.length} sample assets · uploads not yet wired</p>
        </div>
        <button className="rs-btn rs-btn--primary" data-placeholder title="Coming soon">
          <Icons.plus size={16} /> Upload assets
        </button>
      </div>
      <div className="rs-media-grid">
        {ASSETS.map((m) => (
          <div className="rs-media-card" key={m.id}>
            <div
              className="rs-media-cover"
              style={{ background: `linear-gradient(135deg, hsl(${m.hue} 50% 80%), hsl(${m.hue + 18} 45% 62%))` }}
            >
              <span className="rs-media-ext rs-mono">{m.ext}</span>
            </div>
            <div className="rs-media-card-meta">
              <strong title={m.name}>{m.name}</strong>
              <span className="rs-cell-muted rs-mono">{m.dim} · {m.size}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Route `/media` to it**

In `ui/src/App.tsx`, add the import:

```tsx
import { MediaLibrary } from "./screens/MediaLibrary";
```

Replace the line `<Route path="media" element={<Navigate to="/" replace />} />` with:

```tsx
          <Route path="media" element={<MediaLibrary />} />
```

(`Navigate` is still used by other routes — leave the import.)

- [ ] **Step 3: Add `media` to Layout Section + crumbs**

In `ui/src/Layout.tsx`:

Change the `Section` type to include `media`:

```tsx
export type Section = "dashboard" | "content" | "builder" | "settings" | "media";
```

In `sectionFromPath`, add before the `content` check:

```tsx
  if (pathname.startsWith("/media")) return "media";
```

In the crumbs block, add:

```tsx
  else if (section === "media") crumbs = ["Media Library"];
```

- [ ] **Step 4: Add the Media rail item**

In `ui/src/components/shell.tsx`, in `Sidebar`'s `items` array, add after the builder item:

```tsx
    { to: "/media", label: "Media Library", icon: "image" },
```

Confirm `image` is a key in `ui/src/components/icons.tsx`:
Run: `grep -c "image:" ui/src/components/icons.tsx`
Expected: ≥1. If 0, port the `image` icon path from `design/ferrum/icons.jsx`.

In `SecondaryPanel`, the `media` section should render no panel (full-width).
Add at the top of `SecondaryPanel`, alongside the existing dashboard guard:

```tsx
  if (section === "dashboard" || section === "media") return null;
```

- [ ] **Step 5: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add ui/src/screens/MediaLibrary.tsx ui/src/App.tsx ui/src/Layout.tsx ui/src/components/shell.tsx ui/src/components/icons.tsx
git commit -m "feat(ui): media library placeholder screen + rail entry"
```

---

### Task B7: Content-Type Builder header + schema-row styling

**Files:**
- Modify: `ui/src/builder/SchemaEditor.tsx`
- Reference: `design/ferrum/screens.jsx` (ContentTypeBuilder)

Only the **presentational** header + read view aligns to the design; the
existing draft/create/patch machinery is untouched.

- [ ] **Step 1: Read the current SchemaEditor to find the header + field list render points**

Run: `sed -n '1,60p' ui/src/builder/SchemaEditor.tsx`
Identify the header (display name + meta line) and the field-list container.

- [ ] **Step 2: Align the header meta line + empty state**

In `ui/src/builder/SchemaEditor.tsx`, ensure the header renders the design's
meta string and a "Preview API" placeholder button. Locate the header block
(the `<div className="rs-cm-head">` or equivalent) and make the subtitle read:

```tsx
          <p className="rs-cm-sub rs-mono">
            api::{name}.{name} · {fields.length} fields · collection type
          </p>
```

where `name` is the type's api id and `fields` the current field list already in
scope (use the existing variable names from Step 1 — do not introduce new state).

Add, next to the existing Add-field / Save actions, a placeholder:

```tsx
          <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon">
            <Icons.eye size={15} /> Preview API
          </button>
```

Confirm `Icons` is imported in this file; if not, add
`import { Icons } from "../components/icons";`.

- [ ] **Step 3: Confirm the field rows use the schema classes**

The field list should render rows with `rs-schema-row`, a `rs-type-pill` for the
kind, and a `rs-req-tag` when required, matching `design/ferrum/screens.jsx`.
If the current rows use different markup, wrap each row's kind in
`<span className="rs-type-pill">{kindLabel}</span>` and required in
`<span className="rs-req-tag">required</span>` — keeping existing edit/delete
handlers intact. Do not change the staged-drop / draft logic.

- [ ] **Step 4: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 5: Commit**

```bash
git add ui/src/builder/SchemaEditor.tsx
git commit -m "feat(ui): builder header meta + schema-row styling + Preview API placeholder"
```

---

### Task B8: Settings — static API-tokens placeholder

**Files:**
- Modify: `ui/src/screens/Settings.tsx`
- Reference: `design/ferrum/screens.jsx` (Settings)

- [ ] **Step 1: Render the design's static token table as a placeholder**

Replace the body of `ui/src/screens/Settings.tsx` with:

```tsx
import { Icons } from "../components/icons";

const TOKENS = [
  { name: "Production read-only", type: "Read-only", last: "11m ago", key: "rst_live_a91f…c4e2" },
  { name: "Website ISR", type: "Custom", last: "2h ago", key: "rst_live_77b0…91da" },
  { name: "Local dev", type: "Full access", last: "3d ago", key: "rst_test_0c3e…ab19" },
];

export function Settings() {
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>API tokens <span className="rs-preview-pill">preview</span></h1>
          <p className="rs-cm-sub">Token management is not yet wired to the API.</p>
        </div>
        <button className="rs-btn rs-btn--primary" data-placeholder title="Coming soon">
          <Icons.plus size={16} /> Create new token
        </button>
      </div>
      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr><th>Name</th><th>Type</th><th>Token</th><th>Last used</th><th className="rs-col-act"></th></tr>
          </thead>
          <tbody>
            {TOKENS.map((t) => (
              <tr key={t.name}>
                <td className="rs-cell-title"><span className="rs-title-text">{t.name}</span></td>
                <td><span className="rs-type-pill">{t.type}</span></td>
                <td className="rs-mono rs-cell-muted">{t.key}</td>
                <td className="rs-cell-muted">{t.last}</td>
                <td className="rs-col-act">
                  <button className="rs-row-btn" data-placeholder title="Coming soon"><Icons.copy size={16} /></button>
                  <button className="rs-row-btn rs-danger" data-placeholder title="Coming soon"><Icons.trash size={16} /></button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
```

Confirm `copy` is an icon key:
Run: `grep -c "copy:" ui/src/components/icons.tsx`
Expected: ≥1. If 0, port the `copy` icon from `design/ferrum/icons.jsx`.

- [ ] **Step 2: Verify build**

Run: `pnpm -C ui build`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/Settings.tsx ui/src/components/icons.tsx
git commit -m "feat(ui): settings API-tokens static placeholder"
```

---

### Task B9: Manual verification pass

**Files:** none (verification only)

- [ ] **Step 1: Build the UI and run the full stack**

Run: `pnpm -C ui build && docker compose up --build -d`
Then open `http://localhost:8080/studio`, log in with the demo admin key.

- [ ] **Step 2: Walk every screen against the design screenshots**

Verify against `design/screenshots/`:
- Dashboard: hero + 4 stat cards + recent list (real articles) + system panel.
- Content Manager → Articles: status tabs with counts, toolbar, rich rows
  (status badge, author avatar where populated), bulk bar on select, pager.
- Authors / Categories lists render generically.
- Content-Type Builder: header meta line, schema rows with type pills.
- Media Library: placeholder grid + "preview" pill.
- Settings: static token table + "preview" pill.
- Rail shows Home / Content / Builder / Media / Settings; identity is "Admin".
- Placeholders (Filters, Fields, Upload, Create token, single types, components)
  are visibly disabled / marked.

- [ ] **Step 3: Tear down**

Run: `docker compose down`

- [ ] **Step 4: Final workspace check + commit any screenshot notes**

Run: `cargo build --workspace && cargo test --workspace && pnpm -C ui build`
Expected: all green. No commit needed unless fixes were made.

---

## Self-Review Notes

- **Spec coverage:** Seed (A1–A6), Dashboard extras (B4), Content list design
  (B5), Builder (B7), Media placeholder (B6), Single types/Components (B3),
  Settings sub-pages (B3, B8), generic Admin identity (B2), real relations (A3),
  backend bootstrap with `FERRUM_SEED` (A1, A4). All spec sections mapped.
- **Known divergence from design (documented in A2):** article→category is a
  many_to_many in the design but v2.4 only supports many_to_one, so seed wires
  only article→author; categories remain a browsable collection. Media/rich-text
  kinds have no API equivalent → omitted/downgraded to text.
- **Placeholders:** every non-functional control carries `data-placeholder` +
  `title="Coming soon"` or a `rs-preview-pill`; no fake success paths.
