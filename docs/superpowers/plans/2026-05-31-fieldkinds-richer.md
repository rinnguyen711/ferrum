# Phase 2.5 — Richer FieldKinds Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `enum`, `json`, `email`, `url`, `slug` field kinds with kind-appropriate write validation and filter operator surfaces. Append-only enum value evolution via a new `PatchContentType.extend_enum_values` op.

**Architecture:** Each new kind is a `FieldKind` variant. Email/url/slug bind as `text` after format validation; enum binds as `text` with a named CHECK constraint regenerated on extend; json gets a new `BoundValue::Json` variant and binds as `jsonb`. Filter operator whitelist updated per kind. Mirrors phase 2.4 (`Relation`) layering: `Field.kind_meta` carries config, `Field::validate()` dispatches per kind, registry-level checks live in the schema service.

**Tech Stack:** Rust 1.88 (uses `std::sync::LazyLock` from 1.80), axum 0.7, sqlx 0.8 (postgres), `regex` 1, `url` 2, testcontainers.

**Predecessor:** Phase 2.4, last SHA `a2fe9a4`. Spec: [docs/superpowers/specs/2026-05-31-fieldkinds-richer-design.md](../specs/2026-05-31-fieldkinds-richer-design.md).

---

## File Structure

**Modify:**
- `crates/core/Cargo.toml` — add `url = "2"` (already in http; promote to core too).
- `crates/core/src/field.rs` — 5 new `FieldKind` variants, `BoundValue::Json`, `CoerceError::{BadEmail,BadUrl,BadSlug}`, `EnumMeta` parser, `Field::enum_meta()` helper, `FieldError` extensions, `Field::validate()` arms, `BoundValue::from_json` arms.
- `crates/core/src/validators.rs` — NEW. Email/slug regex (via `LazyLock`), url scheme check.
- `crates/core/src/lib.rs` — export `validators`, re-export `EnumMeta`.
- `crates/core/src/error.rs` — new `Error` variants per spec §5.
- `crates/core/src/content_type.rs` — `EnumExtension` struct, `PatchContentType.extend_enum_values`, validation rules.
- `crates/sql/src/ddl.rs` — column emitter branches per kind, named CHECK constraint for enum, new `alter_enum_values` function.
- `crates/sql/src/dml.rs` — bind `BoundValue::Json` as `jsonb`.
- `crates/sql/src/filter.rs` — extend `op_allows_kind` per spec §3.
- `crates/schema/src/service.rs` — `extend_enum_values` execution in `patch`, registry-level validation.
- `crates/http/src/entry.rs` — enum membership check post-coerce, json bind path.
- `crates/http/src/filter.rs` — bypass format validation for filter values per spec §3.1.
- `crates/http/src/error.rs` — HTTP mappings for new `Error` variants.

**Create:**
- `crates/core/src/validators.rs` — validators module.
- `crates/bin/tests/fieldkinds.rs` — integration tests.

**Out of scope (deferred):** uuid user-declarable kind, media_ref, enum value removal/rename, json `$contains`, slug per-parent uniqueness scoping, enum ordering.

---

## Task Sequencing Overview

1. **Task 1:** core — add `url` dep to core crate; `validators.rs` with email/slug regex + url scheme check; unit tests.
2. **Task 2:** core — add 5 `FieldKind` variants + `BoundValue::Json` + `CoerceError` variants; coerce arms for json/email/url/slug; enum coerces to `Str` (membership later).
3. **Task 3:** core — `EnumMeta` parser + `Field::enum_meta()` helper + `FieldError` enum extensions.
4. **Task 4:** core — `Field::validate()` per-kind dispatch arms (enum/json/email/url/slug) including default + unique rules.
5. **Task 5:** core — `EnumExtension` + `PatchContentType.extend_enum_values` + `validate()` rules.
6. **Task 6:** core — new `Error` variants + re-exports.
7. **Task 7:** sql DDL — column emitter branches per kind + named CHECK for enum + `alter_enum_values` function.
8. **Task 8:** sql DML — bind `BoundValue::Json` as jsonb.
9. **Task 9:** sql filter — `op_allows_kind` whitelist per kind.
10. **Task 10:** schema service — execute `extend_enum_values` after add/drop; persist updated `kind_meta`; registry-level validation.
11. **Task 11:** http entry — enum membership check + json bind path + HTTP error mappings.
12. **Task 12:** http filter — bypass format validation for filter values on email/url/slug set ops.
13. **Task 13:** Integration tests (`crates/bin/tests/fieldkinds.rs`).
14. **Task 14:** Workspace verification + done criteria.

---

## Task 1: Core validators module + `url` dep

**Files:**
- Modify: `crates/core/Cargo.toml`
- Create: `crates/core/src/validators.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Add `url` dependency to `crates/core/Cargo.toml`**

Locate `[dependencies]` section. Append after `regex = "1"`:

```toml
url = "2"
```

- [ ] **Step 2: Write failing tests for validators**

Create `crates/core/src/validators.rs` with this content:

```rust
//! Format validators for new field kinds (phase 2.5).
//! Email and slug use compiled regex (LazyLock); url uses the `url` crate.

use std::sync::LazyLock;
use regex::Regex;

static EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap());
static SLUG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap());

pub const SLUG_MAX_LEN: usize = 200;

pub fn is_valid_email(s: &str) -> bool {
    EMAIL_RE.is_match(s)
}

pub fn is_valid_slug(s: &str) -> bool {
    s.len() <= SLUG_MAX_LEN && SLUG_RE.is_match(s)
}

pub fn is_valid_http_url(s: &str) -> bool {
    match url::Url::parse(s) {
        Ok(u) => matches!(u.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_ok() {
        assert!(is_valid_email("a@b.co"));
        assert!(is_valid_email("user.name+tag@example.com"));
    }

    #[test]
    fn email_bad() {
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("no-at-sign"));
        assert!(!is_valid_email("missing@tld"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("white space@b.co"));
    }

    #[test]
    fn slug_ok() {
        assert!(is_valid_slug("hello"));
        assert!(is_valid_slug("hello-world"));
        assert!(is_valid_slug("a1-b2-c3"));
        assert!(is_valid_slug("1"));
    }

    #[test]
    fn slug_bad() {
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("Hello"));
        assert!(!is_valid_slug("-leading"));
        assert!(!is_valid_slug("trailing-"));
        assert!(!is_valid_slug("two--dashes"));
        assert!(!is_valid_slug("under_score"));
    }

    #[test]
    fn slug_too_long() {
        let s: String = std::iter::repeat('a').take(SLUG_MAX_LEN + 1).collect();
        assert!(!is_valid_slug(&s));
    }

    #[test]
    fn url_http_ok() {
        assert!(is_valid_http_url("http://example.com"));
        assert!(is_valid_http_url("https://example.com/path?q=1"));
    }

    #[test]
    fn url_non_http_rejected() {
        assert!(!is_valid_http_url("ftp://example.com"));
        assert!(!is_valid_http_url("mailto:a@b.co"));
        assert!(!is_valid_http_url("javascript:alert(1)"));
    }

    #[test]
    fn url_bad_parse() {
        assert!(!is_valid_http_url(""));
        assert!(!is_valid_http_url("not a url"));
    }
}
```

- [ ] **Step 3: Declare the module in `lib.rs`**

In `crates/core/src/lib.rs`, add near the other `pub mod` declarations:

```rust
pub mod validators;
```

- [ ] **Step 4: Run tests; expect PASS**

Run: `cargo test -p rustapi-core validators::tests`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "$(cat <<'EOF'
feat(core): validators module for email/url/slug

Compiled regex via std::sync::LazyLock (Rust 1.80+). Url validator
restricts scheme to http/https. Foundation for phase 2.5 field kinds.
EOF
)"
```

---

## Task 2: Core — new `FieldKind` variants + `BoundValue::Json` + coercion arms

**Files:**
- Modify: `crates/core/src/field.rs`

- [ ] **Step 1: Write failing tests for new coerce arms**

Append to the existing `mod tests` in `crates/core/src/field.rs`:

```rust
#[test]
fn coerce_json_accepts_any_value() {
    let v = BoundValue::from_json(FieldKind::Json, &serde_json::json!({"k": [1, 2]})).unwrap();
    match v {
        BoundValue::Json(serde_json::Value::Object(_)) => {}
        other => panic!("expected Json(Object), got {other:?}"),
    }
    let v = BoundValue::from_json(FieldKind::Json, &serde_json::json!([1, 2, 3])).unwrap();
    assert!(matches!(v, BoundValue::Json(serde_json::Value::Array(_))));
    let v = BoundValue::from_json(FieldKind::Json, &serde_json::json!(42)).unwrap();
    assert!(matches!(v, BoundValue::Json(_)));
}

#[test]
fn coerce_email_ok() {
    let v = BoundValue::from_json(FieldKind::Email, &serde_json::json!("a@b.co")).unwrap();
    assert!(matches!(v, BoundValue::Str(s) if s == "a@b.co"));
}

#[test]
fn coerce_email_bad() {
    assert_eq!(
        BoundValue::from_json(FieldKind::Email, &serde_json::json!("nope")).unwrap_err(),
        CoerceError::BadEmail
    );
}

#[test]
fn coerce_email_rejects_non_string() {
    assert_eq!(
        BoundValue::from_json(FieldKind::Email, &serde_json::json!(123)).unwrap_err(),
        CoerceError::TypeMismatch
    );
}

#[test]
fn coerce_url_ok() {
    let v = BoundValue::from_json(FieldKind::Url, &serde_json::json!("https://x.io/p")).unwrap();
    assert!(matches!(v, BoundValue::Str(_)));
}

#[test]
fn coerce_url_bad() {
    assert_eq!(
        BoundValue::from_json(FieldKind::Url, &serde_json::json!("ftp://x.io")).unwrap_err(),
        CoerceError::BadUrl
    );
}

#[test]
fn coerce_slug_ok() {
    let v = BoundValue::from_json(FieldKind::Slug, &serde_json::json!("hello-world")).unwrap();
    assert!(matches!(v, BoundValue::Str(s) if s == "hello-world"));
}

#[test]
fn coerce_slug_bad() {
    assert_eq!(
        BoundValue::from_json(FieldKind::Slug, &serde_json::json!("Bad Slug!")).unwrap_err(),
        CoerceError::BadSlug
    );
}

#[test]
fn coerce_enum_returns_str() {
    // Enum coercion does not check membership (no kind_meta access).
    // Service layer validates after coerce. Just confirm it produces Str.
    let v = BoundValue::from_json(FieldKind::Enum, &serde_json::json!("draft")).unwrap();
    assert!(matches!(v, BoundValue::Str(s) if s == "draft"));
}

#[test]
fn coerce_enum_rejects_non_string() {
    assert_eq!(
        BoundValue::from_json(FieldKind::Enum, &serde_json::json!(42)).unwrap_err(),
        CoerceError::TypeMismatch
    );
}
```

- [ ] **Step 2: Run; expect FAIL**

Run: `cargo test -p rustapi-core coerce_json_accepts_any_value coerce_email_ok coerce_url_ok coerce_slug_ok coerce_enum_returns_str`
Expected: FAIL — variants don't exist.

- [ ] **Step 3: Add the new `FieldKind` variants**

In `crates/core/src/field.rs`, extend `enum FieldKind`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum FieldKind {
    String,
    Text,
    Integer,
    Float,
    Boolean,
    Datetime,
    Uuid,
    Relation,
    /// Phase 2.5: closed set of strings. Values declared in `Field.kind_meta`.
    Enum,
    /// Phase 2.5: arbitrary JSON stored as jsonb. No schema validation.
    Json,
    /// Phase 2.5: text validated against an email regex at write time.
    Email,
    /// Phase 2.5: text parsed as an http/https URL at write time.
    Url,
    /// Phase 2.5: text validated against a kebab slug regex at write time.
    Slug,
}
```

- [ ] **Step 4: Extend `BoundValue` and `CoerceError`**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BoundValue {
    Null(FieldKind),
    Str(String),
    I64(i64),
    F64(f64),
    Bool(bool),
    DateTime(DateTime<Utc>),
    Uuid(uuid::Uuid),
    Json(serde_json::Value),
}
```

```rust
#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum CoerceError {
    #[error("type mismatch")]
    TypeMismatch,
    #[error("value out of range")]
    OutOfRange,
    #[error("invalid RFC3339 datetime")]
    BadDatetime,
    #[error("invalid UUID")]
    BadUuid,
    #[error("invalid email")]
    BadEmail,
    #[error("invalid URL (must be http or https)")]
    BadUrl,
    #[error("invalid slug (use lowercase letters, digits, single dashes; <=200 chars)")]
    BadSlug,
}
```

- [ ] **Step 5: Add coerce arms to `BoundValue::from_json`**

Add these arms BEFORE the existing trailing `_ => Err(CoerceError::TypeMismatch)`:

```rust
(FieldKind::Json, v) => Ok(BoundValue::Json(v.clone())),
(FieldKind::Email, V::String(s)) => {
    if crate::validators::is_valid_email(s) {
        Ok(BoundValue::Str(s.clone()))
    } else {
        Err(CoerceError::BadEmail)
    }
}
(FieldKind::Url, V::String(s)) => {
    if crate::validators::is_valid_http_url(s) {
        Ok(BoundValue::Str(s.clone()))
    } else {
        Err(CoerceError::BadUrl)
    }
}
(FieldKind::Slug, V::String(s)) => {
    if crate::validators::is_valid_slug(s) {
        Ok(BoundValue::Str(s.clone()))
    } else {
        Err(CoerceError::BadSlug)
    }
}
(FieldKind::Enum, V::String(s)) => Ok(BoundValue::Str(s.clone())),
```

The `Json` arm is `v` not a destructure — order it FIRST among new arms so it always wins for `FieldKind::Json`. The Enum arm covers strings; non-strings fall through to `TypeMismatch`.

- [ ] **Step 6: Run new tests; expect PASS**

Run: `cargo test -p rustapi-core`
Expected: all tests PASS (existing + new ones).

If existing tests touching `FieldKind` (`null_carries_kind`) iterate variants, they may need updates — verify by running and reading failures.

- [ ] **Step 7: Commit**

```bash
git add crates/core
git commit -m "$(cat <<'EOF'
feat(core): FieldKind variants + BoundValue::Json for phase 2.5

Adds Enum/Json/Email/Url/Slug to FieldKind. Json coercion accepts any
value via new BoundValue::Json. Email/Url/Slug go through validators
module and produce BoundValue::Str. Enum coerces strings to Str with
membership checked at the service layer.
EOF
)"
```

---

## Task 3: Core — `EnumMeta` parser + `Field::enum_meta()` helper + `FieldError` extensions

**Files:**
- Modify: `crates/core/src/field.rs`

- [ ] **Step 1: Write failing tests for `EnumMeta::from_value`**

Append a new module to `crates/core/src/field.rs`:

```rust
#[cfg(test)]
mod enum_meta_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_minimal() {
        let m = EnumMeta::from_value(&json!({"values": ["draft", "published"]})).unwrap();
        assert_eq!(m.values, vec!["draft".to_string(), "published".to_string()]);
    }

    #[test]
    fn reject_missing_values() {
        assert_eq!(
            EnumMeta::from_value(&json!({})).unwrap_err(),
            FieldError::EnumMetaShape
        );
    }

    #[test]
    fn reject_empty_values() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": []})).unwrap_err(),
            FieldError::EnumValuesEmpty
        );
    }

    #[test]
    fn reject_non_string_value() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["a", 1]})).unwrap_err(),
            FieldError::EnumMetaShape
        );
    }

    #[test]
    fn reject_invalid_ident() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["Bad-Value"]})).unwrap_err(),
            FieldError::EnumValueInvalidIdent("Bad-Value".into())
        );
    }

    #[test]
    fn reject_duplicate() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["a", "b", "a"]})).unwrap_err(),
            FieldError::EnumValueDuplicate("a".into())
        );
    }

    #[test]
    fn reject_extra_keys() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["a"], "extra": 1})).unwrap_err(),
            FieldError::EnumMetaShape
        );
    }
}
```

- [ ] **Step 2: Run; expect FAIL**

Run: `cargo test -p rustapi-core enum_meta_tests`
Expected: FAIL — `EnumMeta`, new `FieldError` variants don't exist.

- [ ] **Step 3: Extend `FieldError`**

Find existing `FieldError` enum and append variants:

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FieldError {
    // ...existing variants from phase 2.4...

    #[error("enum kind_meta must be {{values: [..]}} of valid idents")]
    EnumMetaShape,
    #[error("enum values list must contain at least one value")]
    EnumValuesEmpty,
    #[error("enum value `{0}` is not a valid identifier")]
    EnumValueInvalidIdent(String),
    #[error("enum value `{0}` appears more than once")]
    EnumValueDuplicate(String),
    #[error("enum default is not in the values list")]
    EnumDefaultNotInValues,
    #[error("json field cannot be unique")]
    JsonUniqueUnsupported,
}
```

- [ ] **Step 4: Define `EnumMeta` and its parser**

Above the `Field` struct in `crates/core/src/field.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct EnumMeta {
    pub values: Vec<String>,
}

impl EnumMeta {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, FieldError> {
        let obj = v.as_object().ok_or(FieldError::EnumMetaShape)?;
        for key in obj.keys() {
            if key != "values" {
                return Err(FieldError::EnumMetaShape);
            }
        }
        let arr = obj
            .get("values")
            .and_then(|x| x.as_array())
            .ok_or(FieldError::EnumMetaShape)?;
        if arr.is_empty() {
            return Err(FieldError::EnumValuesEmpty);
        }
        let mut values = Vec::with_capacity(arr.len());
        let mut seen = std::collections::HashSet::new();
        for item in arr {
            let s = item.as_str().ok_or(FieldError::EnumMetaShape)?;
            if !crate::reserved::is_valid_ident(s) {
                return Err(FieldError::EnumValueInvalidIdent(s.to_string()));
            }
            if !seen.insert(s.to_string()) {
                return Err(FieldError::EnumValueDuplicate(s.to_string()));
            }
            values.push(s.to_string());
        }
        Ok(Self { values })
    }
}
```

- [ ] **Step 5: Add `Field::enum_meta()` helper**

In the `impl Field` block (near `relation_meta`):

```rust
pub fn enum_meta(&self) -> Option<EnumMeta> {
    if self.kind == FieldKind::Enum {
        EnumMeta::from_value(&self.kind_meta).ok()
    } else {
        None
    }
}
```

- [ ] **Step 6: Run; expect PASS**

Run: `cargo test -p rustapi-core enum_meta_tests`
Expected: 7 passed.

- [ ] **Step 7: Re-export `EnumMeta` from `lib.rs`**

In `crates/core/src/lib.rs`, extend the `pub use field::...` line:

```rust
pub use field::{
    BoundValue, CoerceError, EnumMeta, Field, FieldError, FieldKind, RelationMeta,
};
```

- [ ] **Step 8: Commit**

```bash
git add crates/core
git commit -m "$(cat <<'EOF'
feat(core): EnumMeta parser + FieldError extensions

Parses {values: [ident,...]} with the same identifier rule used for
field names and relation targets. Rejects empty / non-array / non-string
entries, invalid idents, duplicates, and unknown top-level keys.
EOF
)"
```

---

## Task 4: Core — `Field::validate()` per-kind dispatch arms

**Files:**
- Modify: `crates/core/src/field.rs`

- [ ] **Step 1: Write failing tests**

Append to the bottom of `mod tests`:

```rust
#[test]
fn validate_enum_ok() {
    let f = Field {
        name: "status".into(),
        kind: FieldKind::Enum,
        required: false,
        unique: false,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta: serde_json::json!({"values": ["draft", "published"]}),
    };
    assert!(f.validate().is_ok());
}

#[test]
fn validate_enum_with_valid_default() {
    let f = Field {
        name: "status".into(),
        kind: FieldKind::Enum,
        required: false,
        unique: false,
        default: serde_json::Value::String("draft".into()),
        max_length: None,
        kind_meta: serde_json::json!({"values": ["draft", "published"]}),
    };
    assert!(f.validate().is_ok());
}

#[test]
fn validate_enum_default_not_in_values() {
    let f = Field {
        name: "status".into(),
        kind: FieldKind::Enum,
        required: false,
        unique: false,
        default: serde_json::Value::String("missing".into()),
        max_length: None,
        kind_meta: serde_json::json!({"values": ["draft", "published"]}),
    };
    assert_eq!(f.validate().unwrap_err(), FieldError::EnumDefaultNotInValues);
}

#[test]
fn validate_enum_unique_allowed() {
    let f = Field {
        name: "status".into(),
        kind: FieldKind::Enum,
        required: false,
        unique: true,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta: serde_json::json!({"values": ["a", "b"]}),
    };
    assert!(f.validate().is_ok());
}

#[test]
fn validate_json_ok() {
    let f = Field {
        name: "meta".into(),
        kind: FieldKind::Json,
        required: false,
        unique: false,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta: serde_json::json!({}),
    };
    assert!(f.validate().is_ok());
}

#[test]
fn validate_json_with_default() {
    let f = Field {
        name: "meta".into(),
        kind: FieldKind::Json,
        required: false,
        unique: false,
        default: serde_json::json!({"k": 1}),
        max_length: None,
        kind_meta: serde_json::json!({}),
    };
    assert!(f.validate().is_ok());
}

#[test]
fn validate_json_rejects_unique() {
    let f = Field {
        name: "meta".into(),
        kind: FieldKind::Json,
        required: false,
        unique: true,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta: serde_json::json!({}),
    };
    assert_eq!(f.validate().unwrap_err(), FieldError::JsonUniqueUnsupported);
}

#[test]
fn validate_json_rejects_non_empty_kind_meta() {
    let f = Field {
        name: "meta".into(),
        kind: FieldKind::Json,
        required: false,
        unique: false,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta: serde_json::json!({"x": 1}),
    };
    assert_eq!(f.validate().unwrap_err(), FieldError::KindMetaNotEmpty);
}

#[test]
fn validate_email_url_slug_ok() {
    for kind in [FieldKind::Email, FieldKind::Url, FieldKind::Slug] {
        let f = Field {
            name: "x".into(),
            kind,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({}),
        };
        assert!(f.validate().is_ok(), "kind {kind:?} should validate");
    }
}

#[test]
fn validate_email_bad_default() {
    let f = Field {
        name: "e".into(),
        kind: FieldKind::Email,
        required: false,
        unique: false,
        default: serde_json::Value::String("nope".into()),
        max_length: None,
        kind_meta: serde_json::json!({}),
    };
    assert_eq!(f.validate().unwrap_err(), FieldError::BadDefault);
}

#[test]
fn validate_url_good_default() {
    let f = Field {
        name: "u".into(),
        kind: FieldKind::Url,
        required: false,
        unique: false,
        default: serde_json::Value::String("https://example.com".into()),
        max_length: None,
        kind_meta: serde_json::json!({}),
    };
    assert!(f.validate().is_ok());
}

#[test]
fn validate_slug_bad_default() {
    let f = Field {
        name: "s".into(),
        kind: FieldKind::Slug,
        required: false,
        unique: false,
        default: serde_json::Value::String("Bad Slug".into()),
        max_length: None,
        kind_meta: serde_json::json!({}),
    };
    assert_eq!(f.validate().unwrap_err(), FieldError::BadDefault);
}
```

- [ ] **Step 2: Run; expect FAIL**

Run: `cargo test -p rustapi-core validate_enum_ok validate_json_ok validate_email_url_slug_ok`
Expected: FAIL — dispatch arms not yet added.

- [ ] **Step 3: Extend `Field::validate()`**

Replace the body of `Field::validate()` to add the new branches. Insert AFTER the existing `FieldKind::Relation` branch and BEFORE the default primitive branch:

```rust
if self.kind == FieldKind::Enum {
    let meta = EnumMeta::from_value(&self.kind_meta)?;
    if !self.default.is_null() {
        match self.default.as_str() {
            Some(s) if meta.values.iter().any(|v| v == s) => {}
            _ => return Err(FieldError::EnumDefaultNotInValues),
        }
    }
    return Ok(());
}
if self.kind == FieldKind::Json {
    if self.unique {
        return Err(FieldError::JsonUniqueUnsupported);
    }
    if !is_empty_obj(&self.kind_meta) {
        return Err(FieldError::KindMetaNotEmpty);
    }
    // Any JSON value is a valid default (including null, but null is the
    // "no default" sentinel here — accept anything else verbatim).
    return Ok(());
}
if matches!(
    self.kind,
    FieldKind::Email | FieldKind::Url | FieldKind::Slug
) {
    if !is_empty_obj(&self.kind_meta) {
        return Err(FieldError::KindMetaNotEmpty);
    }
    if !self.default.is_null() {
        BoundValue::from_json(self.kind, &self.default)
            .map_err(|_| FieldError::BadDefault)?;
    }
    return Ok(());
}
```

The existing tail (primitive validation) stays untouched.

- [ ] **Step 4: Run tests; expect PASS**

Run: `cargo test -p rustapi-core`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "$(cat <<'EOF'
feat(core): Field::validate dispatches enum/json/email/url/slug

Enum requires valid EnumMeta + (if defaulted) default in values.
Json rejects unique + must keep kind_meta empty. Email/Url/Slug
validate default via from_json. Other kinds untouched.
EOF
)"
```

---

## Task 5: Core — `EnumExtension` + `PatchContentType.extend_enum_values`

**Files:**
- Modify: `crates/core/src/content_type.rs`

- [ ] **Step 1: Inspect existing `PatchContentType`**

Run: `grep -nE 'struct PatchContentType|impl PatchContentType' crates/core/src/content_type.rs | head -5`
Note structure shape and `validate()` location.

- [ ] **Step 2: Write failing tests**

Append to the existing test module in `crates/core/src/content_type.rs`:

```rust
#[test]
fn patch_extend_enum_values_ok() {
    let existing = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({"values": ["draft", "published"]}),
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let p = PatchContentType {
        display_name: None,
        add_fields: vec![],
        drop_fields: vec![],
        extend_enum_values: vec![EnumExtension {
            field: "status".into(),
            append: vec!["archived".into()],
        }],
    };
    assert!(p.validate(&existing).is_ok());
}

#[test]
fn patch_extend_enum_values_unknown_field() {
    let existing = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let p = PatchContentType {
        display_name: None,
        add_fields: vec![],
        drop_fields: vec![],
        extend_enum_values: vec![EnumExtension {
            field: "status".into(),
            append: vec!["x".into()],
        }],
    };
    let err = p.validate(&existing).unwrap_err();
    assert!(format!("{err:?}").contains("EnumExtendUnknownField"));
}

#[test]
fn patch_extend_enum_values_not_enum_field() {
    let existing = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "title".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({}),
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let p = PatchContentType {
        display_name: None,
        add_fields: vec![],
        drop_fields: vec![],
        extend_enum_values: vec![EnumExtension {
            field: "title".into(),
            append: vec!["x".into()],
        }],
    };
    let err = p.validate(&existing).unwrap_err();
    assert!(format!("{err:?}").contains("EnumExtendNotEnum"));
}

#[test]
fn patch_extend_enum_values_duplicate_against_existing() {
    let existing = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({"values": ["draft", "published"]}),
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let p = PatchContentType {
        display_name: None,
        add_fields: vec![],
        drop_fields: vec![],
        extend_enum_values: vec![EnumExtension {
            field: "status".into(),
            append: vec!["draft".into()],
        }],
    };
    let err = p.validate(&existing).unwrap_err();
    assert!(format!("{err:?}").contains("EnumValueDuplicate"));
}

#[test]
fn patch_extend_enum_values_empty_append() {
    let existing = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({"values": ["draft"]}),
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let p = PatchContentType {
        display_name: None,
        add_fields: vec![],
        drop_fields: vec![],
        extend_enum_values: vec![EnumExtension {
            field: "status".into(),
            append: vec![],
        }],
    };
    let err = p.validate(&existing).unwrap_err();
    assert!(format!("{err:?}").contains("EnumValuesEmpty"));
}

#[test]
fn patch_extend_enum_values_conflict_with_drop() {
    let existing = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({"values": ["draft"]}),
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let p = PatchContentType {
        display_name: None,
        add_fields: vec![],
        drop_fields: vec!["status".into()],
        extend_enum_values: vec![EnumExtension {
            field: "status".into(),
            append: vec!["archived".into()],
        }],
    };
    let err = p.validate(&existing).unwrap_err();
    assert!(format!("{err:?}").contains("EnumExtendConflictWithAddDrop"));
}
```

- [ ] **Step 3: Run; expect FAIL**

Run: `cargo test -p rustapi-core patch_extend_enum_values`
Expected: FAIL — struct field absent.

- [ ] **Step 4: Define `EnumExtension` + extend `PatchContentType`**

In `crates/core/src/content_type.rs`, add above `PatchContentType`:

```rust
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EnumExtension {
    pub field: String,
    pub append: Vec<String>,
}
```

Extend `PatchContentType`:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PatchContentType {
    pub display_name: Option<String>,
    #[serde(default)]
    pub add_fields: Vec<Field>,
    #[serde(default)]
    pub drop_fields: Vec<String>,
    #[serde(default)]
    pub extend_enum_values: Vec<EnumExtension>,
}
```

Adjust the `is_empty` check (if any) — currently the `validate` body checks `display_name.is_none() && add_fields.is_empty() && drop_fields.is_empty()`. Extend it:

```rust
if self.display_name.is_none()
    && self.add_fields.is_empty()
    && self.drop_fields.is_empty()
    && self.extend_enum_values.is_empty()
{
    return Err(ContentTypeError::EmptyPatch);
}
```

(Use the actual variant name from the existing code.)

- [ ] **Step 5: Add `ContentTypeError` variants**

In the same file, extend the error enum:

```rust
#[error("extend_enum_values references unknown field `{0}`")]
EnumExtendUnknownField(String),
#[error("extend_enum_values targets non-enum field `{0}`")]
EnumExtendNotEnum(String),
#[error("field `{0}` is both modified via drop/add and extend_enum_values in the same patch")]
EnumExtendConflictWithAddDrop(String),
```

`EnumValuesEmpty` and `EnumValueInvalidIdent` and `EnumValueDuplicate` already exist on `FieldError` (Task 3) — surface them here as `ContentTypeError::Field(FieldError)` if a variant for that exists, else wrap via `From`. If `ContentTypeError` has no field-error pass-through, add one:

```rust
#[error("{0}")]
Field(#[from] FieldError),
```

- [ ] **Step 6: Implement `extend_enum_values` validation**

Inside `PatchContentType::validate(&self, existing: &ContentType) -> Result<(), ContentTypeError>`, after the existing add/drop logic, add:

```rust
let drop_set: std::collections::HashSet<&str> =
    self.drop_fields.iter().map(|s| s.as_str()).collect();
let add_set: std::collections::HashSet<&str> =
    self.add_fields.iter().map(|f| f.name.as_str()).collect();

for ext in &self.extend_enum_values {
    if drop_set.contains(ext.field.as_str()) || add_set.contains(ext.field.as_str()) {
        return Err(ContentTypeError::EnumExtendConflictWithAddDrop(ext.field.clone()));
    }
    let target = existing
        .fields
        .iter()
        .find(|f| f.name == ext.field)
        .ok_or_else(|| ContentTypeError::EnumExtendUnknownField(ext.field.clone()))?;
    if target.kind != FieldKind::Enum {
        return Err(ContentTypeError::EnumExtendNotEnum(ext.field.clone()));
    }
    if ext.append.is_empty() {
        return Err(FieldError::EnumValuesEmpty.into());
    }
    let existing_meta = target
        .enum_meta()
        .ok_or_else(|| ContentTypeError::EnumExtendNotEnum(ext.field.clone()))?;
    let existing_set: std::collections::HashSet<&str> =
        existing_meta.values.iter().map(|s| s.as_str()).collect();
    let mut new_seen = std::collections::HashSet::new();
    for v in &ext.append {
        if !crate::reserved::is_valid_ident(v) {
            return Err(FieldError::EnumValueInvalidIdent(v.clone()).into());
        }
        if existing_set.contains(v.as_str()) || !new_seen.insert(v.as_str()) {
            return Err(FieldError::EnumValueDuplicate(v.clone()).into());
        }
    }
}
```

- [ ] **Step 7: Run tests; expect PASS**

Run: `cargo test -p rustapi-core patch_extend_enum_values`
Expected: 6 passed.

- [ ] **Step 8: Re-export `EnumExtension` from `lib.rs`**

Add `EnumExtension` to the `pub use content_type::...` re-export line.

- [ ] **Step 9: Commit**

```bash
git add crates/core
git commit -m "$(cat <<'EOF'
feat(core): PatchContentType.extend_enum_values append op

Append-only schema evolution for enum fields. Validates target exists,
target is enum, no conflict with drop/add of the same field, append
list non-empty, each new value a valid ident, no overlap with existing
values, no internal duplicates.
EOF
)"
```

---

## Task 6: Core — new `Error` variants

**Files:**
- Modify: `crates/core/src/error.rs`

- [ ] **Step 1: Add variants per spec §5**

Locate the public `Error` enum. Append:

```rust
#[error("enum value `{value}` not allowed for field `{field}`")]
EnumValueNotAllowed {
    field: String,
    value: String,
    allowed: Vec<String>,
},
#[error("invalid email")]
BadEmail,
#[error("invalid URL")]
BadUrl,
#[error("invalid slug")]
BadSlug,
```

The `EnumExtend*` errors live in `ContentTypeError` (raised by validate) and surface to the public `Error` via the existing `From<ContentTypeError>` impl (assume present from phase 2.4; verify). If absent, also add:

```rust
#[error("{0}")]
ContentType(#[from] crate::content_type::ContentTypeError),
```

- [ ] **Step 2: Build (no new tests yet — covered by HTTP-layer tests)**

Run: `cargo build -p rustapi-core`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add crates/core
git commit -m "$(cat <<'EOF'
feat(core): Error variants for phase 2.5 fieldkinds

EnumValueNotAllowed (with allowed list for UX), BadEmail/BadUrl/BadSlug
coerce surfaces. HTTP layer maps to 422 in a later task.
EOF
)"
```

---

## Task 7: SQL DDL — column emitter branches + named CHECK + `alter_enum_values`

**Files:**
- Modify: `crates/sql/src/ddl.rs`

- [ ] **Step 1: Locate the column emitter**

Run: `grep -nE 'fn (create_table|add_column|column_def)' crates/sql/src/ddl.rs`
Note the per-column emitter (likely `column_def`).

- [ ] **Step 2: Write failing tests**

Append to the test module in `crates/sql/src/ddl.rs`:

```rust
#[test]
fn create_table_emits_enum_check() {
    use rustapi_core::{ContentType, Field, FieldKind};
    use chrono::Utc;
    use serde_json::json;
    let ct = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"values": ["draft", "published"]}),
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let sql = create_table(&ct).unwrap();
    assert!(sql.contains("\"status\" text"));
    assert!(sql.contains("CONSTRAINT \"post_status_enum_chk\""));
    assert!(sql.contains("CHECK (\"status\" IS NULL OR \"status\" IN ('draft', 'published'))"));
}

#[test]
fn create_table_emits_json_jsonb() {
    use rustapi_core::{ContentType, Field, FieldKind};
    use chrono::Utc;
    use serde_json::json;
    let ct = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "meta".into(),
            kind: FieldKind::Json,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({}),
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let sql = create_table(&ct).unwrap();
    assert!(sql.contains("\"meta\" jsonb"));
}

#[test]
fn create_table_emits_text_for_email_url_slug() {
    use rustapi_core::{ContentType, Field, FieldKind};
    use chrono::Utc;
    use serde_json::json;
    for (kind, name) in [
        (FieldKind::Email, "e"),
        (FieldKind::Url, "u"),
        (FieldKind::Slug, "s"),
    ] {
        let ct = ContentType {
            id: uuid::Uuid::new_v4(),
            name: "x".into(),
            display_name: "X".into(),
            fields: vec![Field {
                name: name.into(),
                kind,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let sql = create_table(&ct).unwrap();
        assert!(sql.contains(&format!("\"{name}\" text")), "{kind:?}");
    }
}

#[test]
fn alter_enum_values_emits_drop_and_add() {
    let sql = alter_enum_values("post", "status", &[
        "draft".to_string(),
        "published".to_string(),
        "archived".to_string(),
    ])
    .unwrap();
    assert!(sql.contains("DROP CONSTRAINT \"post_status_enum_chk\""));
    assert!(sql.contains("ADD CONSTRAINT \"post_status_enum_chk\""));
    assert!(sql.contains("'draft', 'published', 'archived'"));
}
```

- [ ] **Step 3: Run; expect FAIL**

Run: `cargo test -p rustapi-sql create_table_emits_enum create_table_emits_json alter_enum_values`
Expected: FAIL.

- [ ] **Step 4: Implement emitter branches**

Locate the existing column emitter. Add branches:

```rust
if f.kind == rustapi_core::FieldKind::Enum {
    let meta = f
        .enum_meta()
        .ok_or_else(|| DdlError::Invalid("enum field missing kind_meta".into()))?;
    let col = quote_ident(&f.name);
    let not_null = if f.required { " NOT NULL" } else { "" };
    let values_lit = meta
        .values
        .iter()
        .map(|v| format!("'{}'", v.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    let constraint_name = quote_ident(&format!("{}_{}_enum_chk", table_name, f.name));
    return Ok(format!(
        "{col} text{not_null} CONSTRAINT {constraint_name} CHECK ({col} IS NULL OR {col} IN ({values_lit}))"
    ));
}
if f.kind == rustapi_core::FieldKind::Json {
    let col = quote_ident(&f.name);
    let not_null = if f.required { " NOT NULL" } else { "" };
    return Ok(format!("{col} jsonb{not_null}"));
}
if matches!(
    f.kind,
    rustapi_core::FieldKind::Email
        | rustapi_core::FieldKind::Url
        | rustapi_core::FieldKind::Slug
) {
    let col = quote_ident(&f.name);
    let not_null = if f.required { " NOT NULL" } else { "" };
    return Ok(format!("{col} text{not_null}"));
}
```

Adjust signature for `table_name` access — if the existing emitter signature does not take `&table_name`, change it to take `table_name: &str` (and update call sites in `create_table` and `add_column`). The CHECK constraint name embeds the table.

- [ ] **Step 5: Implement `alter_enum_values`**

Append to `crates/sql/src/ddl.rs`:

```rust
pub fn alter_enum_values(
    table: &str,
    col: &str,
    all_values: &[String],
) -> Result<String, DdlError> {
    let table_q = quote_ident(table);
    let col_q = quote_ident(col);
    let constraint_q = quote_ident(&format!("{table}_{col}_enum_chk"));
    let values_lit = all_values
        .iter()
        .map(|v| format!("'{}'", v.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "ALTER TABLE {table_q} DROP CONSTRAINT {constraint_q}; \
         ALTER TABLE {table_q} ADD CONSTRAINT {constraint_q} \
         CHECK ({col_q} IS NULL OR {col_q} IN ({values_lit}))"
    ))
}
```

- [ ] **Step 6: Update `add_column`**

If `add_column` builds its own SQL, mirror the per-kind branches (or call into the shared emitter). Make sure enum ADD COLUMN includes the named CHECK constraint.

- [ ] **Step 7: Run tests; expect PASS**

Run: `cargo test -p rustapi-sql`
Expected: PASS for new + existing.

- [ ] **Step 8: Commit**

```bash
git add crates/sql
git commit -m "$(cat <<'EOF'
feat(sql): DDL emitters for enum/json/email/url/slug

Enum: text + named CHECK (`<table>_<col>_enum_chk`). Json: jsonb.
Email/url/slug: text. New alter_enum_values() function emits paired
DROP+ADD constraint statements for append-only enum evolution.
EOF
)"
```

---

## Task 8: SQL DML — bind `BoundValue::Json` as jsonb

**Files:**
- Modify: `crates/sql/src/dml.rs`

- [ ] **Step 1: Locate the bind helper**

Run: `grep -nE 'BoundValue::|bind_all|fn bind' crates/sql/src/dml.rs | head -20`
Find the function that maps `BoundValue` variants onto the sqlx query builder.

- [ ] **Step 2: Add `Json` arm**

In the bind site, add an arm:

```rust
BoundValue::Json(v) => q.bind(sqlx::types::Json(v.clone())),
```

(or whatever the existing arms look like — match style). `sqlx::types::Json<T>` Encode-impls onto `jsonb` automatically. If `sqlx::types::Json` is unavailable, use `q.bind(v)` directly; sqlx's `Value` Encode lands on jsonb.

If `bind_all` lives in `crates/http`, the change there instead — search both.

- [ ] **Step 3: Run sql + http tests**

Run: `cargo test -p rustapi-sql && cargo test -p rustapi-http`
Expected: PASS, no new regressions.

- [ ] **Step 4: Commit**

```bash
git add crates
git commit -m "$(cat <<'EOF'
feat(sql): bind BoundValue::Json as jsonb

Json variant binds via sqlx::types::Json for explicit jsonb encoding.
EOF
)"
```

---

## Task 9: SQL filter — `op_allows_kind` per kind

**Files:**
- Modify: `crates/sql/src/filter.rs`

- [ ] **Step 1: Locate `op_allows_kind`**

Run: `grep -nE 'fn op_allows_kind|FilterOp' crates/sql/src/filter.rs | head -10`

- [ ] **Step 2: Write failing tests**

Append to the filter test module:

```rust
#[test]
fn op_allows_kind_enum() {
    use rustapi_core::FieldKind::Enum;
    assert!(op_allows_kind(FilterOp::Eq, Enum));
    assert!(op_allows_kind(FilterOp::Ne, Enum));
    assert!(op_allows_kind(FilterOp::Null, Enum));
    assert!(op_allows_kind(FilterOp::In, Enum));
    assert!(op_allows_kind(FilterOp::Nin, Enum));
    assert!(!op_allows_kind(FilterOp::Contains, Enum));
    assert!(!op_allows_kind(FilterOp::Gt, Enum));
}

#[test]
fn op_allows_kind_json_null_only() {
    use rustapi_core::FieldKind::Json;
    assert!(op_allows_kind(FilterOp::Null, Json));
    assert!(!op_allows_kind(FilterOp::Eq, Json));
    assert!(!op_allows_kind(FilterOp::Contains, Json));
    assert!(!op_allows_kind(FilterOp::In, Json));
}

#[test]
fn op_allows_kind_email_url_slug_full_string() {
    use rustapi_core::FieldKind;
    for kind in [FieldKind::Email, FieldKind::Url, FieldKind::Slug] {
        assert!(op_allows_kind(FilterOp::Eq, kind));
        assert!(op_allows_kind(FilterOp::Ne, kind));
        assert!(op_allows_kind(FilterOp::Null, kind));
        assert!(op_allows_kind(FilterOp::In, kind));
        assert!(op_allows_kind(FilterOp::Nin, kind));
        assert!(op_allows_kind(FilterOp::Contains, kind));
        assert!(op_allows_kind(FilterOp::StartsWith, kind));
        assert!(op_allows_kind(FilterOp::EndsWith, kind));
        assert!(!op_allows_kind(FilterOp::Gt, kind));
        assert!(!op_allows_kind(FilterOp::Lt, kind));
    }
}
```

(Use the actual `FilterOp` variant names; the names above mirror what phase 2.3 introduced.)

- [ ] **Step 3: Run; expect FAIL**

Run: `cargo test -p rustapi-sql op_allows_kind_enum op_allows_kind_json`
Expected: FAIL.

- [ ] **Step 4: Extend `op_allows_kind`**

Modify the function so the new kinds slot in. Likely structure today is a big match by op then by kind. Add arms per the table in spec §3. Example sketch:

```rust
use rustapi_core::FieldKind as K;
match op {
    FilterOp::Eq | FilterOp::Ne => matches!(
        kind,
        K::String | K::Text | K::Integer | K::Float | K::Boolean | K::Datetime
            | K::Uuid | K::Email | K::Url | K::Slug | K::Enum
    ),
    FilterOp::Null => true, // null is universal
    FilterOp::In | FilterOp::Nin => matches!(
        kind,
        K::String | K::Text | K::Integer | K::Float | K::Datetime
            | K::Uuid | K::Email | K::Url | K::Slug | K::Enum
    ),
    FilterOp::Contains | FilterOp::StartsWith | FilterOp::EndsWith => matches!(
        kind,
        K::String | K::Text | K::Email | K::Url | K::Slug
    ),
    FilterOp::Gt | FilterOp::Gte | FilterOp::Lt | FilterOp::Lte => matches!(
        kind,
        K::Integer | K::Float | K::Datetime
    ),
}
```

Keep relation Uuid kind handling consistent with phase 2.4 — relation fields are remapped to `Uuid` at the HTTP layer before this function sees them. Json appears in none of these branches except `Null` (covered by the `Null => true` arm).

- [ ] **Step 5: Run tests**

Run: `cargo test -p rustapi-sql`
Expected: all PASS (new + existing). Watch for fallout in existing phase 2.1-2.4 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/sql
git commit -m "$(cat <<'EOF'
feat(sql): op_allows_kind covers enum/json/email/url/slug

Enum/email/url/slug get $eq/$ne/$in/$nin (+ string ops for the latter
three). Json restricted to $null. Relation remains routed through Uuid.
EOF
)"
```

---

## Task 10: Schema service — execute `extend_enum_values`, persist meta

**Files:**
- Modify: `crates/schema/src/service.rs`

- [ ] **Step 1: Locate `SchemaService::patch`**

Run: `grep -nE 'fn patch|extend_enum_values|update.*content_type' crates/schema/src/service.rs | head -10`

- [ ] **Step 2: Add DDL execution path**

In `SchemaService::patch`, after add/drop DDL completes (in the same transaction), execute `extend_enum_values` for each extension:

```rust
for ext in &payload.extend_enum_values {
    let target = updated_ct
        .fields
        .iter_mut()
        .find(|f| f.name == ext.field)
        .expect("validated to exist");
    let mut meta = target
        .enum_meta()
        .expect("validated to be enum");
    meta.values.extend(ext.append.iter().cloned());
    let sql = rustapi_sql::ddl::alter_enum_values(
        &updated_ct.name,
        &ext.field,
        &meta.values,
    )?;
    sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
    target.kind_meta = serde_json::json!({"values": meta.values});
}
```

`updated_ct` is the in-memory ContentType being mutated to reflect the patch — match how existing add/drop branches mutate it.

- [ ] **Step 3: Persist the updated `content_types` row**

The existing patch flow already writes back the canonical row after add/drop. Confirm the `fields` column is serialized after the loop above so `kind_meta` updates persist.

- [ ] **Step 4: Reload registry**

If the existing flow ends with `registry.insert(updated_ct.clone())` or similar, no change needed. Otherwise ensure the in-memory registry reflects the new values.

- [ ] **Step 5: Add unit test (no DB) for validation pass-through**

Append to `mod tests` in `crates/schema/src/service.rs`:

```rust
#[tokio::test]
async fn validate_extend_enum_values_unknown_field() {
    // SchemaService::patch validates via PatchContentType::validate
    // before touching the DB. A unit test reusing the in-memory
    // registry path confirms the error surface.
    // (If patch is hard-bound to PgPool, skip this and rely on the
    // integration tests in Task 13.)
}
```

If patch is hard-bound to pool, leave this as a comment and rely on Task 13 integration coverage. Don't fight the architecture.

- [ ] **Step 6: Run schema tests**

Run: `cargo test -p rustapi-schema`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/schema
git commit -m "$(cat <<'EOF'
feat(schema): SchemaService.patch runs extend_enum_values

After add/drop DDL, each extension fires ALTER TABLE DROP CONSTRAINT
+ ADD CONSTRAINT with the union of existing + appended values, then
updates the in-memory kind_meta so the persisted row reflects them.
EOF
)"
```

---

## Task 11: HTTP entry — enum membership check, json bind, error mappings

**Files:**
- Modify: `crates/http/src/entry.rs`
- Modify: `crates/http/src/error.rs`

- [ ] **Step 1: Locate the write-path coerce loop**

Run: `grep -nE 'BoundValue::from_json|FieldKind::Relation' crates/http/src/entry.rs | head -10`
Note where coercion happens.

- [ ] **Step 2: Add enum membership check after coerce**

Inside the field loop, after `BoundValue::from_json(...)` succeeds for an enum field, check membership:

```rust
if f.kind == FieldKind::Enum {
    let value_str = match &bound {
        BoundValue::Str(s) => s.clone(),
        BoundValue::Null(_) => {
            // null is allowed when not required; coercion already handled this.
            // (Service-level null-on-required check is the existing path.)
            String::new()
        }
        _ => unreachable!("enum coerces to Str or Null"),
    };
    if !matches!(bound, BoundValue::Null(_)) {
        let meta = f.enum_meta().ok_or_else(|| {
            ApiError(Error::Internal(anyhow::anyhow!(
                "enum field missing meta at coerce time"
            )))
        })?;
        if !meta.values.iter().any(|v| v == &value_str) {
            return Err(ApiError(Error::EnumValueNotAllowed {
                field: f.name.clone(),
                value: value_str,
                allowed: meta.values.clone(),
            }));
        }
    }
}
```

Adjust to fit the loop structure. The Json variant binds through unchanged (Task 8 handled the SQL side).

- [ ] **Step 3: Add HTTP error mappings**

In `crates/http/src/error.rs`, extend the `into_response` arms:

```rust
Error::EnumValueNotAllowed { field, value, allowed } => {
    let body = serde_json::json!({
        "error": {
            "code": "enum_value_not_allowed",
            "message": format!("value `{value}` not allowed for `{field}`"),
            "details": {"field": field, "value": value, "allowed": allowed}
        }
    });
    (StatusCode::UNPROCESSABLE_ENTITY, Json(body)).into_response()
}
Error::BadEmail => simple_422("bad_email", "invalid email"),
Error::BadUrl => simple_422("bad_url", "invalid URL"),
Error::BadSlug => simple_422("bad_slug", "invalid slug"),
```

Use whatever the existing 422 helper is (`simple_422` is a placeholder for the actual helper). Match style.

- [ ] **Step 4: Map `CoerceError::BadEmail/BadUrl/BadSlug` at the entry handler**

When `BoundValue::from_json` returns these, the entry handler's existing `CoerceError` → `Error` conversion needs new arms. Find the mapping (likely a `match` on `CoerceError`):

```rust
CoerceError::BadEmail => Error::BadEmail,
CoerceError::BadUrl => Error::BadUrl,
CoerceError::BadSlug => Error::BadSlug,
```

- [ ] **Step 5: Add error.rs unit tests**

```rust
#[test]
fn enum_value_not_allowed_is_422() {
    let e = Error::EnumValueNotAllowed {
        field: "status".into(),
        value: "bad".into(),
        allowed: vec!["draft".into()],
    };
    let resp = ApiError(e).into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn bad_email_is_422() {
    let resp = ApiError(Error::BadEmail).into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}
```

- [ ] **Step 6: Run http tests**

Run: `cargo test -p rustapi-http`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/http
git commit -m "$(cat <<'EOF'
feat(http): enum membership check + new error mappings

Entry write-path validates enum membership after coerce; surfaces
EnumValueNotAllowed with the allowed list in details. BadEmail/BadUrl/
BadSlug from CoerceError map to dedicated 422 responses.
EOF
)"
```

---

## Task 12: HTTP filter — bypass format validation for filter values

**Files:**
- Modify: `crates/http/src/filter.rs`

- [ ] **Step 1: Locate the filter value coerce site**

Run: `grep -nE 'BoundValue::from_json|coerce.*value' crates/http/src/filter.rs | head -10`

- [ ] **Step 2: Write failing tests**

Append to the filter test module:

```rust
#[test]
fn email_eq_accepts_non_email_filter_value() {
    use rustapi_core::{ContentType, Field, FieldKind};
    use chrono::Utc;
    use serde_json::json;
    let ct = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "user".into(),
        display_name: "User".into(),
        fields: vec![Field {
            name: "email".into(),
            kind: FieldKind::Email,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({}),
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    // A filter value that would FAIL email coercion at write time should
    // still parse at filter time (raw string compare).
    let q = "filter[email][$contains]=not-an-email";
    assert!(parse_filter(&ct, q).is_ok());
    let q2 = "filter[email][$eq]=not-an-email";
    assert!(parse_filter(&ct, q2).is_ok());
}

#[test]
fn slug_startswith_accepts_non_slug_filter_value() {
    use rustapi_core::{ContentType, Field, FieldKind};
    use chrono::Utc;
    use serde_json::json;
    let ct = ContentType {
        id: uuid::Uuid::new_v4(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![Field {
            name: "slug".into(),
            kind: FieldKind::Slug,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({}),
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let q = "filter[slug][$startsWith]=Some Random String";
    assert!(parse_filter(&ct, q).is_ok());
}
```

- [ ] **Step 3: Run; expect FAIL (current behavior coerces and rejects)**

Run: `cargo test -p rustapi-http email_eq_accepts_non_email slug_startswith_accepts`
Expected: FAIL.

- [ ] **Step 4: Bypass format validation for filter values**

In the filter parse site, when coercing the value, treat email/url/slug as raw `FieldKind::String` for the coercion call. Example:

```rust
let coerce_kind = match field.kind {
    rustapi_core::FieldKind::Email
    | rustapi_core::FieldKind::Url
    | rustapi_core::FieldKind::Slug => rustapi_core::FieldKind::String,
    other => other,
};
let bound = rustapi_core::BoundValue::from_json(coerce_kind, &raw_value)?;
```

The SQL column is still the actual field's column; only the coerce kind changes. The bound value lands as `BoundValue::Str(raw_value_as_string)` and binds to the text column normally.

For enum filters, same treatment — accept any string (does NOT validate membership). Add it to the `match` arms:

```rust
| rustapi_core::FieldKind::Enum
```

Json fields: filter parse already rejects everything except `$null` via `op_allows_kind`, so no special coerce path needed.

- [ ] **Step 5: Run tests**

Run: `cargo test -p rustapi-http`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http
git commit -m "$(cat <<'EOF'
feat(http): filter values bypass format validation for new kinds

Email/url/slug/enum filter values coerce as raw strings so users can
query legacy or malformed data and so $contains/$startsWith make sense
on substrings. Write-path validation is unchanged.
EOF
)"
```

---

## Task 13: Integration tests (testcontainers)

**Files:**
- Create: `crates/bin/tests/fieldkinds.rs`

- [ ] **Step 1: Bootstrap the test file**

Create `crates/bin/tests/fieldkinds.rs`:

```rust
mod common;
use common::TestApp;
use serde_json::{json, Value};
```

- [ ] **Step 2: Add type-creation helpers**

```rust
async fn create_post_with_status_enum(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "status", "kind": "enum",
                 "kind_meta": {"values": ["draft", "published"]}}
            ]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_doc_with_json(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "doc",
            "display_name": "Doc",
            "fields": [
                {"name": "meta", "kind": "json"}
            ]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_user_with_email(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "user",
            "display_name": "User",
            "fields": [
                {"name": "email", "kind": "email"}
            ]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_link_with_url(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "link",
            "display_name": "Link",
            "fields": [
                {"name": "url", "kind": "url"}
            ]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_article_with_slug(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [
                {"name": "slug", "kind": "slug", "unique": true}
            ]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}
```

- [ ] **Step 3: Write the integration tests**

Add one `#[tokio::test]` per case below. Pattern: spawn app, create type, perform action, assert status + body.

1. `post_enum_valid_value_returns_201`
2. `post_enum_invalid_value_returns_422_with_allowed`
3. `filter_enum_eq_matches`
4. `filter_enum_in_returns_union`
5. `patch_extend_enum_values_adds_value_writable`
6. `patch_extend_enum_values_on_non_enum_returns_422`
7. `patch_extend_enum_values_duplicate_returns_422`
8. `patch_extend_enum_values_empty_returns_422`
9. `post_required_enum_null_returns_422` (fixture w/ required:true)
10. `patch_add_required_enum_field_returns_422`
11. `post_json_accepts_nested_object`
12. `filter_json_null_true_matches_null_rows`
13. `filter_json_null_false_matches_nonnull_rows`
14. `filter_json_eq_returns_422_or_validation_error`
15. `post_email_valid_returns_201`
16. `post_email_invalid_returns_422_bad_email`
17. `filter_email_contains_raw_substring`
18. `filter_email_eq_exact_match`
19. `post_url_valid_https_returns_201`
20. `post_url_ftp_returns_422`
21. `filter_url_startsWith_works`
22. `post_slug_valid_returns_201`
23. `post_slug_invalid_returns_422`
24. `filter_slug_eq_exact_match`
25. `slug_unique_violation_returns_409`
26. `mixed_entry_with_all_new_kinds_plus_relation_round_trips`

For #26, build a type with one of each new kind plus a relation field pointing to another type, POST a row, GET it back, assert every field round-trips correctly.

Each test follows the pattern in `crates/bin/tests/relations.rs` from phase 2.4 — spawn, create type(s), POST/GET/PATCH/DELETE, assert status + body. Copy idioms verbatim where possible.

- [ ] **Step 4: Run the integration tests**

Run: `cargo test -p rustapi --test fieldkinds`
Expected: all 26 PASS.

If a test reveals a real behavior bug (rather than a test bug), STOP and report. Don't paper over with relaxed assertions.

- [ ] **Step 5: Commit**

```bash
git add crates/bin
git commit -m "$(cat <<'EOF'
test(bin): integration coverage for phase 2.5 fieldkinds

26 testcontainers cases for enum/json/email/url/slug write validation,
filter operator surfaces, append-only enum evolution, and a
mixed-kind round-trip including a relation field.
EOF
)"
```

---

## Task 14: Workspace verification + done criteria

- [ ] **Step 1: Full workspace test run**

Run: `cargo test --workspace`
Expected: total green; ~370 tests (baseline 326 + ~45 new).

- [ ] **Step 2: Clippy**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean. Address every warning at the source (no `#[allow]` unless justified inline).

- [ ] **Step 3: Phase 2.1-2.4 regression check**

Run: `cargo test --workspace 2>&1 | grep -E '^test result'`
Spot-check: counts match expected baselines for each previous-phase test binary.

- [ ] **Step 4: Final commit (only if README updated)**

Plan README has no per-phase status table (verified during phase 2.4). Skip.

- [ ] **Step 5: Invoke superpowers:finishing-a-development-branch**

That skill walks through merge/PR options.

---

## Done Criteria Checklist

- [ ] All 14 tasks committed sequentially on top of `a2fe9a4`.
- [ ] `cargo test --workspace` green; ~370 tests.
- [ ] `cargo clippy --all-targets -- -Dwarnings` clean.
- [ ] Phase 2.1-2.4 integration tests unchanged.
- [ ] `git log --oneline` shows discrete, focused commits per task.
- [ ] Spec §Out-of-scope items remain unimplemented (no scope creep).
