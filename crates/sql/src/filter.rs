//! Filter expressions. Phase 2.1 shipped `$eq` / `$ne` / `$null` combined with
//! implicit AND. Phase 2.2 added order / set / string operators. Phase 2.3 adds
//! recursive combinators (`$or`, `$and`, `$not`) — `Filter` is now a tree.

use rustapi_core::{BoundValue, FieldKind};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
    /// Implicit AND across children. Empty vec is treated as `None` by the
    /// emitter. Single-child vecs are elided (no redundant parens).
    All(Vec<Filter>),
    /// Logical OR across children. Empty vec is rejected by the parser; the
    /// emitter has a defensive guard.
    Any(Vec<Filter>),
    /// Logical NOT. Unary by construction (parser enforces).
    Not(Box<Filter>),
    /// Terminal leaf — a single column condition.
    Leaf(Condition),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Condition {
    /// Already validated as an identifier by upstream callers. The SQL emitter
    /// re-validates via `quote_ident`.
    pub column: String,
    /// Column kind, used by `render_where` to pick the right Postgres cast.
    pub kind: FieldKind,
    pub op: Op,
    pub value: FilterValue,
}

impl Condition {
    pub fn new(column: impl Into<String>, kind: FieldKind, op: Op, value: FilterValue) -> Self {
        Self {
            column: column.into(),
            kind,
            op,
            value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Op {
    // Phase 2.1
    Eq,
    Ne,
    IsNull,
    // Phase 2.2 — order
    Gt,
    Gte,
    Lt,
    Lte,
    // Phase 2.2 — set
    In,
    NotIn,
    // Phase 2.2 — string
    Contains,
    StartsWith,
    EndsWith,
    ContainsI,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilterValue {
    /// Used by `$eq` / `$ne` / order ops / string ops. When the inner
    /// `BoundValue` is `Null(kind)` the emitter rewrites `Eq`/`Ne` to
    /// `IS NULL` / `IS NOT NULL` (phase 2.1 behavior).
    Bound(BoundValue),
    /// Used by `$null`: true = IS NULL, false = IS NOT NULL.
    Null(bool),
    /// Used by `$in` / `$nin`. Empty list is rejected by the parser
    /// AND defensively re-checked by `render_where`.
    List(Vec<BoundValue>),
}

/// True iff `op` is meaningful for `kind`. The parser enforces the rejection;
/// the emitter trusts this contract.
pub fn op_allows_kind(op: Op, kind: FieldKind) -> bool {
    use FieldKind::*;
    use Op::*;
    if kind == FieldKind::Component {
        return false;
    }
    match op {
        Eq | Ne => matches!(
            kind,
            String | Text | Integer | Float | Boolean | Datetime | Uuid | Email | Url | Slug | Enum
        ),
        IsNull => true,
        Gt | Gte | Lt | Lte => matches!(kind, Integer | Float | Datetime),
        In | NotIn => matches!(
            kind,
            String | Text | Integer | Float | Boolean | Datetime | Uuid | Email | Url | Slug | Enum
        ),
        Contains | StartsWith | EndsWith | ContainsI => {
            matches!(kind, String | Text | Email | Url | Slug)
        } // Same-crate `#[non_exhaustive]` does not require a wildcard. If a
          // new `Op` lands in this crate, the build breaks here until the
          // matrix gets an explicit entry — intentional safety.
    }
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

    #[test]
    fn op_allows_kind_order_only_on_numeric_and_datetime() {
        for kind in [FieldKind::Integer, FieldKind::Float, FieldKind::Datetime] {
            assert!(op_allows_kind(Op::Gt, kind));
            assert!(op_allows_kind(Op::Gte, kind));
            assert!(op_allows_kind(Op::Lt, kind));
            assert!(op_allows_kind(Op::Lte, kind));
        }
        for kind in [
            FieldKind::String,
            FieldKind::Text,
            FieldKind::Boolean,
            FieldKind::Uuid,
        ] {
            assert!(!op_allows_kind(Op::Gt, kind));
            assert!(!op_allows_kind(Op::Lt, kind));
        }
    }

    #[test]
    fn op_allows_kind_string_ops_only_on_string_kinds() {
        for kind in [FieldKind::String, FieldKind::Text] {
            assert!(op_allows_kind(Op::Contains, kind));
            assert!(op_allows_kind(Op::StartsWith, kind));
            assert!(op_allows_kind(Op::EndsWith, kind));
            assert!(op_allows_kind(Op::ContainsI, kind));
        }
        for kind in [
            FieldKind::Integer,
            FieldKind::Float,
            FieldKind::Boolean,
            FieldKind::Datetime,
            FieldKind::Uuid,
        ] {
            assert!(!op_allows_kind(Op::Contains, kind));
            assert!(!op_allows_kind(Op::ContainsI, kind));
        }
    }

    #[test]
    fn op_allows_kind_set_ops_on_every_kind() {
        for kind in [
            FieldKind::String,
            FieldKind::Text,
            FieldKind::Integer,
            FieldKind::Float,
            FieldKind::Boolean,
            FieldKind::Datetime,
            FieldKind::Uuid,
        ] {
            assert!(op_allows_kind(Op::In, kind));
            assert!(op_allows_kind(Op::NotIn, kind));
        }
    }

    #[test]
    fn op_allows_kind_phase_2_1_ops_unchanged() {
        for kind in [
            FieldKind::String,
            FieldKind::Text,
            FieldKind::Integer,
            FieldKind::Float,
            FieldKind::Boolean,
            FieldKind::Datetime,
            FieldKind::Uuid,
        ] {
            assert!(op_allows_kind(Op::Eq, kind));
            assert!(op_allows_kind(Op::Ne, kind));
            assert!(op_allows_kind(Op::IsNull, kind));
        }
    }

    // Phase 2.3 — recursive variants.

    #[test]
    fn leaf_variant_holds_condition() {
        let c = Condition::new("title", FieldKind::String, Op::Eq, FilterValue::Null(true));
        let f = Filter::Leaf(c.clone());
        let Filter::Leaf(inner) = f else {
            panic!("expected Leaf")
        };
        assert_eq!(inner.column, c.column);
    }

    #[test]
    fn any_variant_holds_vec() {
        let f = Filter::Any(vec![
            Filter::Leaf(Condition::new(
                "a",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Null(true),
            )),
            Filter::Leaf(Condition::new(
                "b",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Null(true),
            )),
        ]);
        let Filter::Any(xs) = f else {
            panic!("expected Any")
        };
        assert_eq!(xs.len(), 2);
    }

    #[test]
    fn not_variant_holds_box() {
        let f = Filter::Not(Box::new(Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Null(true),
        ))));
        let Filter::Not(inner) = f else {
            panic!("expected Not")
        };
        assert!(matches!(*inner, Filter::Leaf(_)));
    }

    #[test]
    fn all_variant_holds_vec_of_filter() {
        let f = Filter::All(vec![
            Filter::Leaf(Condition::new(
                "a",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Null(true),
            )),
            Filter::Any(vec![Filter::Leaf(Condition::new(
                "b",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Null(true),
            ))]),
        ]);
        let Filter::All(xs) = f else {
            panic!("expected All")
        };
        assert_eq!(xs.len(), 2);
        assert!(matches!(xs[0], Filter::Leaf(_)));
        assert!(matches!(xs[1], Filter::Any(_)));
    }

    #[test]
    fn op_allows_kind_enum() {
        use FieldKind::Enum;
        assert!(op_allows_kind(Op::Eq, Enum));
        assert!(op_allows_kind(Op::Ne, Enum));
        assert!(op_allows_kind(Op::IsNull, Enum));
        assert!(op_allows_kind(Op::In, Enum));
        assert!(op_allows_kind(Op::NotIn, Enum));
        assert!(!op_allows_kind(Op::Contains, Enum));
        assert!(!op_allows_kind(Op::StartsWith, Enum));
        assert!(!op_allows_kind(Op::Gt, Enum));
        assert!(!op_allows_kind(Op::Lt, Enum));
    }

    #[test]
    fn op_allows_kind_json_isnull_only() {
        use FieldKind::Json;
        assert!(op_allows_kind(Op::IsNull, Json));
        assert!(!op_allows_kind(Op::Eq, Json));
        assert!(!op_allows_kind(Op::Ne, Json));
        assert!(!op_allows_kind(Op::Contains, Json));
        assert!(!op_allows_kind(Op::In, Json));
        assert!(!op_allows_kind(Op::Gt, Json));
    }

    #[test]
    fn op_allows_kind_email_url_slug_full_string() {
        for kind in [FieldKind::Email, FieldKind::Url, FieldKind::Slug] {
            assert!(op_allows_kind(Op::Eq, kind), "{kind:?}");
            assert!(op_allows_kind(Op::Ne, kind), "{kind:?}");
            assert!(op_allows_kind(Op::IsNull, kind), "{kind:?}");
            assert!(op_allows_kind(Op::In, kind), "{kind:?}");
            assert!(op_allows_kind(Op::NotIn, kind), "{kind:?}");
            assert!(op_allows_kind(Op::Contains, kind), "{kind:?}");
            assert!(op_allows_kind(Op::StartsWith, kind), "{kind:?}");
            assert!(op_allows_kind(Op::EndsWith, kind), "{kind:?}");
            assert!(op_allows_kind(Op::ContainsI, kind), "{kind:?}");
            assert!(!op_allows_kind(Op::Gt, kind), "{kind:?}");
            assert!(!op_allows_kind(Op::Lt, kind), "{kind:?}");
        }
    }
}
