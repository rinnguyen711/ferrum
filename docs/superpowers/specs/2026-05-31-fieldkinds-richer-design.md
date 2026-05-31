# Phase 2.5 — Richer FieldKinds Design

**Date:** 2026-05-31
**Predecessor:** Phase 2.4 (relations), last SHA `d2aa751`.
**Goal:** Add five new field kinds — `enum`, `json`, `email`, `url`, `slug` — to the v1 type system. Each kind enforces format at write time and exposes a kind-appropriate filter operator surface.

**Out of scope (deferred):** `uuid` user-declarable kind, `media_ref` (depends on phase 6 media storage), enum value removal/rename, json `$contains` / jsonpath, slug uniqueness scoped to parent FK, enum ordering semantics.

---

## 1. FieldKind additions

| Kind | PG storage | `kind_meta` shape | Notes |
|---|---|---|---|
| `Enum` | `text` + named `CHECK` constraint | `{values: ["a", "b", ...]}` | Values must be `^[a-z][a-z0-9_]{0,62}$`. ≥1 value required. Distinct. |
| `Json` | `jsonb` | `{}` | Accepts any JSON. No schema validation. |
| `Email` | `text` | `{}` | Format: `^[^@\s]+@[^@\s]+\.[^@\s]+$`. |
| `Url` | `text` | `{}` | Parsed via `url::Url::parse`. Scheme must be `http` or `https`. |
| `Slug` | `text` | `{}` | Format: `^[a-z0-9]+(-[a-z0-9]+)*$`. Max 200 chars. |

Nullability across all five: column is nullable iff `Field.required == false`. `max_length` ignored for enum/json/email/url/slug — these have intrinsic limits or no length.

### CHECK constraint for enum

```sql
ALTER TABLE "<table>"
  ADD CONSTRAINT "<table>_<col>_enum_chk"
  CHECK ("<col>" IS NULL OR "<col>" IN ('a', 'b', 'c'));
```

Constraint name pattern: `<table>_<col>_enum_chk`. Regenerated on `extend_enum_values` PATCH (see §4) via `DROP CONSTRAINT ... ; ADD CONSTRAINT ...` in the same transaction.

---

## 2. Write-path validation

### 2.1 `BoundValue` and `CoerceError`

New variants:

```rust
pub enum BoundValue {
    // ...existing...
    Json(serde_json::Value),
}

pub enum CoerceError {
    // ...existing...
    BadEmail,
    BadUrl,
    BadSlug,
}
```

### 2.2 Coercion arms (`BoundValue::from_json`)

| Kind | JSON shape accepted | Result |
|---|---|---|
| `Json` | any `Value` | `BoundValue::Json(v.clone())` |
| `Email` | `String` matching email regex | `BoundValue::Str(s)` |
| `Url` | `String` parsing as `http(s)://...` | `BoundValue::Str(s)` |
| `Slug` | `String` matching slug regex, ≤200 chars | `BoundValue::Str(s)` |
| `Enum` | `String` (membership checked later) | `BoundValue::Str(s)` |

Enum membership is **not** checked in coercion — `from_json` has no access to `kind_meta`. The HTTP entry handler reads `f.enum_meta().values` after coerce and rejects with `Error::EnumValueNotAllowed { field, value, allowed }` when not a member.

### 2.3 Validators module

New file `crates/core/src/validators.rs`:

```rust
use once_cell::sync::Lazy;
use regex::Regex;

pub static EMAIL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap());
pub static SLUG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap());
pub const SLUG_MAX_LEN: usize = 200;

pub fn is_valid_email(s: &str) -> bool { EMAIL_RE.is_match(s) }
pub fn is_valid_slug(s: &str) -> bool { s.len() <= SLUG_MAX_LEN && SLUG_RE.is_match(s) }
pub fn is_valid_http_url(s: &str) -> bool {
    match url::Url::parse(s) {
        Ok(u) => matches!(u.scheme(), "http" | "https"),
        Err(_) => false,
    }
}
```

`once_cell` and `regex` are workspace deps (verify or add). `url` crate added at workspace level.

### 2.4 `EnumMeta` parser

Mirrors `RelationMeta` precedent:

```rust
pub struct EnumMeta { pub values: Vec<String> }

impl EnumMeta {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, FieldError> {
        // - object with single key "values"
        // - values = non-empty array of strings
        // - each value: ^[a-z][a-z0-9_]{0,62}$
        // - no duplicates
        // - reject unknown top-level keys
    }
}
```

`FieldError` extends with:

```rust
EnumMetaShape,
EnumValueInvalidIdent(String),
EnumValueDuplicate(String),
EnumValuesEmpty,
EnumDefaultNotInValues,
JsonUniqueUnsupported,
```

### 2.5 `Field::validate()` dispatch

Existing dispatch already branches on `kind == Relation`. Extend:

- `FieldKind::Enum` → `EnumMeta::from_value(&self.kind_meta)?`; `unique` allowed (one row per enum value is a meaningful pattern, e.g. singleton config rows); if `default` is non-null it must be a string in the enum values set (else `FieldError::EnumDefaultNotInValues`).
- `FieldKind::Json` → `kind_meta` must be empty `{}`; reject `unique` with `FieldError::JsonUniqueUnsupported` (uniqueness on free-form JSON is semantically dubious and adds index complexity for negligible value); `default` is the JSON value verbatim (no further validation).
- `FieldKind::Email | Url | Slug` → `kind_meta` empty; default (if non-null) must pass kind's validator; honor `unique` flag verbatim.

### 2.6 Enum default validation

If `Field.default` is non-null on an enum field, it must be a string AND in the values set. Surface as `FieldError::EnumDefaultNotInValues`.

---

## 3. Filter operators per kind

Update `op_allows_kind(op, kind) -> bool` in `crates/sql/src/filter.rs`:

| Op | Enum | Json | Email | Url | Slug |
|---|:-:|:-:|:-:|:-:|:-:|
| `$eq` / `$ne` | ✅ | ❌ | ✅ | ✅ | ✅ |
| `$null` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `$in` / `$nin` | ✅ | ❌ | ✅ | ✅ | ✅ |
| `$contains` / `$startsWith` / `$endsWith` | ❌ | ❌ | ✅ | ✅ | ✅ |
| `$gt` / `$gte` / `$lt` / `$lte` | ❌ | ❌ | ❌ | ❌ | ❌ |

### 3.1 Coercion for filter values

Email, Url, Slug: when the op is `$eq` / `$ne` / `$in` / `$nin`, **skip format validation** on the filter value. Substring ops are raw string compares. Rationale: users querying legacy/malformed data shouldn't be locked out.

Enum: filter `$eq`/`$ne`/`$in`/`$nin` accepts any string (does NOT validate membership). Useful for `$ne` with arbitrary value, and consistent with the relaxed coercion above.

Json: only `$null` is accepted; any other op surfaces as the existing `Error::Validation` op-kind mismatch (422). No new dedicated `JsonOpUnsupported` variant — funnels through the same `Error::Validation` path that relation `$gt` uses (deliberate precedent from phase 2.4 task 7).

---

## 4. Schema evolution

### 4.1 `PatchContentType` shape extension

Existing fields: `display_name: Option<String>`, `add_fields: Vec<Field>`, `drop_fields: Vec<String>`. Add:

```rust
pub struct EnumExtension {
    pub field: String,
    pub append: Vec<String>,
}

pub struct PatchContentType {
    // ...existing...
    #[serde(default)]
    pub extend_enum_values: Vec<EnumExtension>,
}
```

`PatchContentType::is_empty()` check updated to consider `extend_enum_values`.

### 4.2 `PatchContentType::validate(existing)` rules for `extend_enum_values`

For each `EnumExtension { field, append }`:

1. `field` must exist on `existing` (after applying `drop_fields`) AND must be `FieldKind::Enum` → else `EnumExtendUnknownField` or `EnumExtendNotEnum`.
2. `append` must be non-empty, each value a valid ident, no duplicates within `append`, no overlap with existing enum values → else `EnumValueInvalidIdent` / `EnumValueDuplicate`.
3. Conflict with `add_fields`/`drop_fields` on same name → reject; same patch can't both add/drop and extend the same field.

### 4.3 DDL emission for `extend_enum_values`

`crates/sql/src/ddl.rs` new function:

```rust
pub fn alter_enum_values(table: &str, col: &str, all_values: &[String]) -> Result<String, DdlError>;
```

Returns:

```sql
ALTER TABLE "<table>" DROP CONSTRAINT "<table>_<col>_enum_chk";
ALTER TABLE "<table>" ADD CONSTRAINT "<table>_<col>_enum_chk"
  CHECK ("<col>" IS NULL OR "<col>" IN ('v1', 'v2', ..., 'vN'));
```

Executed sequentially in the same `SchemaService::patch` transaction.

### 4.4 Registry / metadata update

After DDL succeeds, the in-memory `ContentType.fields[i].kind_meta` for the target enum field is updated to the new values list. Persisted to `content_types` table alongside other PATCH writes.

### 4.5 Other kinds: no special PATCH path

`Json`, `Email`, `Url`, `Slug` follow existing string/relation patch rules: `add_fields` accepts them (must be nullable on add — same as relations). `drop_fields` works unchanged. No kind-specific patch ops.

---

## 5. Error taxonomy

New `rustapi_core::Error` variants:

| Variant | HTTP status | Body details |
|---|---|---|
| `EnumValueNotAllowed { field, value, allowed }` | 422 | `{field, value, allowed}` |
| `EnumDefaultNotInValues { field, default }` | 422 | `{field, default}` |
| `EnumExtendUnknownField(String)` | 422 | `{field}` |
| `EnumExtendNotEnum(String)` | 422 | `{field}` |
| `EnumExtendConflictWithAddDrop(String)` | 422 | `{field}` |
| `BadEmail` | 422 | — |
| `BadUrl` | 422 | — |
| `BadSlug` | 422 | — |

`FieldError` (core-internal) gets: `EnumMetaShape`, `EnumValueInvalidIdent(String)`, `EnumValueDuplicate(String)`, `EnumValuesEmpty`, `EnumDefaultNotInValues`. These bubble up to `Error::Validation(ValidationErrors)` at the service boundary (existing pattern).

Json op unsupported: no dedicated variant — `op_allows_kind` returning false routes through the existing op-kind mismatch path → `Error::Validation` → 422.

---

## 6. Crate touchpoints

### `rustapi-core`
- `field.rs`: 5 new `FieldKind` variants, `BoundValue::Json`, new `CoerceError` variants, `EnumMeta` type + parser, `Field::validate()` dispatch arms, `Field::enum_meta()` accessor.
- `validators.rs` (new): regex / url validators.
- `content_type.rs`: `EnumExtension` struct, `PatchContentType.extend_enum_values`, validation rules.
- `error.rs`: new `Error` variants.
- `lib.rs`: re-exports.
- `Cargo.toml`: add `regex`, `once_cell`, `url` deps (workspace-level if missing).

### `rustapi-sql`
- `ddl.rs`: column emitters for enum (`text` + CHECK), json (`jsonb`), email/url/slug (`text`). New `alter_enum_values()` function. `add_column` branches per kind.
- `dml.rs`: bind `BoundValue::Json` as jsonb (`.bind::<sqlx::types::Json<&Value>>(...)` or raw value bind). Other new kinds bind as text via existing `Str` arm.
- `filter.rs`: extend `op_allows_kind` per §3.

### `rustapi-schema`
- `service.rs`: `SchemaService::patch` executes `extend_enum_values` DDL after add/drop. Updates persisted `kind_meta`. New `validate_enum_extensions_cross_refs` helper for registry-level checks.
- `registry.rs`: no changes (`SchemaRegistry` already serializes `kind_meta` from DB).

### `rustapi-http`
- `entry.rs`: enum membership check post-coerce. Json bind in write path.
- `filter.rs`: bypass format validation for filter values per §3.1.
- `error.rs`: HTTP status mappings for new `Error` variants.

### `rustapi` (bin)
- `tests/fieldkinds.rs` (new): integration coverage (see §7).

---

## 7. Testing

### Core unit (~25)

- `BoundValue::from_json` happy + failure per kind (10 tests)
- Email/url/slug validator (6 tests)
- `EnumMeta::from_value` ok / missing values / empty values / bad ident / duplicate / extra keys (6 tests)
- `Field::validate` per new kind: ok cases + invalid `kind_meta` + bad default + unique restriction (8 tests)
- `PatchContentType::validate` for `extend_enum_values` (4 tests)

### SQL unit (~10)

- `create_table` emits CHECK for enum (with and without `NOT NULL`)
- `create_table` emits `jsonb` for json
- `create_table` emits `text` for email/url/slug
- `add_column` per kind (nullable)
- `alter_enum_values` emits DROP + ADD
- DML insert/update binds Json variant
- DML insert emits correct column names (regression)

### Schema service (~5)

- `extend_enum_values` succeeds end-to-end
- Reject unknown field / non-enum field / duplicate / conflicting add+extend
- Registry reflects new values after PATCH

### HTTP unit (~5)

- `op_allows_kind` per new kind (table-driven)
- Filter coerce skips format check on email/url/slug for set ops
- `Error` → status mappings

### Integration (`crates/bin/tests/fieldkinds.rs`, ~25)

1. Create enum field, write valid value, read back
2. Write invalid enum value → 422 with `{allowed}` in details
3. Filter enum `$eq` matches
4. Filter enum `$in` returns union
5. PATCH `extend_enum_values` appends, new value writable
6. PATCH `extend_enum_values` on non-enum field → 422
7. PATCH `extend_enum_values` duplicate (against existing) → 422
8. PATCH `extend_enum_values` empty append → 422
9. Required enum field, null value → 422
10. PATCH add required enum field → 422 (no backfill)
11. Create json field, write nested object, read back
12. Json `$null=true` matches null rows
13. Json `$null=false` matches non-null rows
14. Json `$eq` → 422
15. Create email field, write valid → 201
16. Bad email → 422
17. Email `$contains` raw substring (accepts non-email value)
18. Email `$eq` exact match
19. Create url field, valid http/https → 201
20. Url `ftp://` → 422
21. Url `$startsWith` works
22. Create slug field, valid + invalid → 201/422
23. Slug `$eq` exact match
24. Slug `unique: true` enforced (PG unique constraint surfaces 409)
25. Mixed entry: write all 5 new kinds + a relation in one POST

### Done criteria

- `cargo test --workspace` green; ~370 tests total (baseline 326 + ~45).
- `cargo clippy --all-targets -- -Dwarnings` clean.
- Phase 2.1–2.4 integration tests untouched.
- Discrete commits per task.

---

## 8. Open Questions

None at sign-off. Implementation plan will surface concrete sub-tasks per crate.
