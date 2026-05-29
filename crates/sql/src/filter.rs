//! Filter expressions. v1 shipped `None` only. Phase 2.1 adds equality + null
//! ops combined with implicit AND. Combinators (OR / NOT) land in phase 2.3.

use rustapi_core::{BoundValue, FieldKind};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
    /// Implicit AND across conditions. An empty vec behaves like `None`.
    All(Vec<Condition>),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Condition {
    /// Already validated as an identifier by upstream callers. The SQL emitter
    /// re-validates via `quote_ident`.
    pub column: String,
    /// Column kind, used by `render_where` to pick the right Postgres cast
    /// for the placeholder. The parser fills this from the schema or the
    /// `SYSTEM_COLUMNS` table; `BoundValue` alone can't disambiguate a Uuid
    /// column from a String column at emission time.
    pub kind: FieldKind,
    pub op: Op,
    pub value: FilterValue,
}

impl Condition {
    pub fn new(column: impl Into<String>, kind: FieldKind, op: Op, value: FilterValue) -> Self {
        Self { column: column.into(), kind, op, value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Op {
    Eq,
    Ne,
    IsNull,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilterValue {
    /// Used by `$eq` / `$ne`. When the inner `BoundValue` is `Null(kind)` the
    /// emitter rewrites to `IS NULL` / `IS NOT NULL`.
    Bound(BoundValue),
    /// Used by `$null`: true = IS NULL, false = IS NOT NULL.
    Null(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none() {
        assert!(matches!(Filter::default(), Filter::None));
    }

    #[test]
    fn condition_new_builds_struct() {
        let c = Condition::new("title", FieldKind::String, Op::Eq, FilterValue::Null(true));
        assert_eq!(c.column, "title");
        assert_eq!(c.kind, FieldKind::String);
        assert_eq!(c.op, Op::Eq);
    }
}
