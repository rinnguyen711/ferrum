//! Strapi-style `?filters[col][$op]=value` parser. Produces a `rustapi_sql::Filter`
//! ready for the SQL builder. v1 supports `$eq`, `$ne`, `$null` with implicit
//! AND across params.

use rustapi_core::{is_system_column, BoundValue, ContentType, Error, Field, FieldKind, ValidationErrors, SYSTEM_COLUMNS};
use rustapi_sql::{Condition, Filter, FilterValue, Op};
use std::collections::HashSet;
use std::sync::OnceLock;
use url::form_urlencoded;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Segment {
    /// `$or`, `$and`, `$not`
    Combinator(String),
    /// `$eq`, `$ne`, `$null`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`,
    /// `$contains`, `$startsWith`, `$endsWith`, `$containsi`
    Op(String),
    /// Group child index (`$or[0]`, `$and[2]`) or set-value index (`$in[3]`).
    Index(usize),
    /// Column name.
    Name(String),
}

/// Split a `filters[...]...` key into ordered `Segment`s. Performs no
/// semantic validation — that's the tree builder's job.
#[allow(dead_code)]
pub(crate) fn tokenize_key(k: &str) -> Result<Vec<Segment>, Error> {
    let rest = k.strip_prefix("filters").ok_or_else(|| {
        Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}` (missing `filters` prefix)"
        )))
    })?;

    let mut segments = Vec::new();
    let mut cur = rest;
    while !cur.is_empty() {
        let inner = cur
            .strip_prefix('[')
            .and_then(|s| {
                let close = s.find(']')?;
                Some((&s[..close], &s[close + 1..]))
            })
            .ok_or_else(|| {
                Error::Validation(ValidationErrors::single(format!(
                    "malformed filter param `{k}` (unbalanced brackets)"
                )))
            })?;
        let (raw, tail) = inner;
        if raw.is_empty() {
            return Err(Error::Validation(ValidationErrors::single(format!(
                "malformed filter param `{k}` (empty bracket)"
            ))));
        }
        let seg = classify_segment(raw);
        segments.push(seg);
        cur = tail;
    }
    if segments.is_empty() {
        return Err(Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}` (no segments)"
        ))));
    }
    Ok(segments)
}

#[allow(dead_code)]
fn classify_segment(raw: &str) -> Segment {
    match raw {
        "$or" | "$and" | "$not" => Segment::Combinator(raw.to_string()),
        s if s.starts_with('$') => Segment::Op(s.to_string()),
        s => match s.parse::<usize>() {
            Ok(n) => Segment::Index(n),
            Err(_) => Segment::Name(s.to_string()),
        },
    }
}

/// Parse a raw query string into a `Filter`. Non-filter params are ignored.
/// Returns `Filter::None` if no filter params are present.
pub fn parse(raw_query: &str, ct: &ContentType) -> Result<Filter, Error> {
    use std::collections::BTreeMap;
    let mut seen: HashSet<(String, Op)> = HashSet::new();
    let mut conds: Vec<Condition> = Vec::new();
    let mut set_buckets: std::collections::HashMap<(String, Op), BTreeMap<usize, BoundValue>> =
        std::collections::HashMap::new();
    let mut set_kinds: std::collections::HashMap<(String, Op), FieldKind> =
        std::collections::HashMap::new();

    for (k, v) in form_urlencoded::parse(raw_query.as_bytes()) {
        if !k.starts_with("filters[") {
            continue;
        }
        let (col, op_str, idx) = parse_key(&k)?;
        let op = map_op(&op_str, &col)?;
        let field = field_for(ct, &col)?;
        let kind = field.kind();

        if !rustapi_sql::op_allows_kind(op, kind) {
            return Err(field_err(
                &col,
                format!("operator `{op_str}` invalid for kind `{kind:?}`"),
            ));
        }

        let is_set_op = matches!(op, Op::In | Op::NotIn);
        match (is_set_op, idx) {
            (true, None) => {
                return Err(field_err(&col, "set operator requires bracketed list indices"));
            }
            (false, Some(_)) => {
                return Err(field_err(&col, "unexpected list index for operator"));
            }
            (true, Some(i)) => {
                if v.eq_ignore_ascii_case("null") {
                    return Err(field_err(&col, "set operator entries cannot be null"));
                }
                let bv = coerce_bound(kind, &col, &v)?;
                let bucket = set_buckets.entry((col.clone(), op)).or_default();
                if bucket.insert(i, bv).is_some() {
                    return Err(field_err(&col, "duplicate set operator entry"));
                }
                set_kinds.insert((col.clone(), op), kind);
                if bucket.len() > 100 {
                    return Err(field_err(&col, "set operator limited to 100 items"));
                }
            }
            (false, None) => {
                if !seen.insert((col.clone(), op)) {
                    return Err(field_err(&col, "duplicate filter operator on column"));
                }
                let value = coerce_value(field, op, &col, &v)?;
                conds.push(Condition::new(col, kind, op, value));
            }
        }
    }

    for ((col, op), bucket) in set_buckets {
        if bucket.is_empty() {
            return Err(field_err(&col, "set operator requires non-empty list"));
        }
        let kind = set_kinds[&(col.clone(), op)];
        let values: Vec<BoundValue> = bucket.into_values().collect();
        conds.push(Condition::new(col, kind, op, FilterValue::List(values)));
    }

    if conds.is_empty() {
        Ok(Filter::None)
    } else {
        Ok(Filter::All(conds.into_iter().map(Filter::Leaf).collect()))
    }
}

fn map_op(op_str: &str, col: &str) -> Result<Op, Error> {
    Ok(match op_str {
        "$eq" => Op::Eq,
        "$ne" => Op::Ne,
        "$null" => Op::IsNull,
        "$gt" => Op::Gt,
        "$gte" => Op::Gte,
        "$lt" => Op::Lt,
        "$lte" => Op::Lte,
        "$in" => Op::In,
        "$nin" => Op::NotIn,
        "$contains" => Op::Contains,
        "$startsWith" => Op::StartsWith,
        "$endsWith" => Op::EndsWith,
        "$containsi" => Op::ContainsI,
        other => return Err(field_err(col, format!("unknown operator `{other}`"))),
    })
}

fn parse_key(k: &str) -> Result<(String, String, Option<usize>), Error> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"^filters\[(?P<col>[^\[\]]+)\]\[(?P<op>\$[a-zA-Z]+)\](?:\[(?P<idx>\d+)\])?$",
        )
        .unwrap()
    });
    let caps = re.captures(k).ok_or_else(|| {
        Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}`"
        )))
    })?;
    let idx = caps
        .name("idx")
        .map(|m| m.as_str().parse::<usize>().expect("regex \\d+ already validated"));
    Ok((caps["col"].to_string(), caps["op"].to_string(), idx))
}

fn field_for<'a>(ct: &'a ContentType, col: &str) -> Result<FieldOrSystem<'a>, Error> {
    if is_system_column(col) {
        return Ok(FieldOrSystem::System(system_kind(col)));
    }
    if let Some(f) = ct.fields.iter().find(|f| f.name == col) {
        return Ok(FieldOrSystem::User(f));
    }
    Err(Error::Validation(ValidationErrors::field(
        col,
        "unknown filter field",
    )))
}

enum FieldOrSystem<'a> {
    User(&'a Field),
    System(FieldKind),
}

impl FieldOrSystem<'_> {
    fn kind(&self) -> FieldKind {
        match self {
            FieldOrSystem::User(f) => f.kind,
            FieldOrSystem::System(k) => *k,
        }
    }
}

fn system_kind(col: &str) -> FieldKind {
    // Pull from the central SYSTEM_COLUMNS table so future additions don't
    // need to be mirrored here. Falls back to Text for unknown columns,
    // which is_system_column should never let through.
    SYSTEM_COLUMNS
        .iter()
        .find(|c| c.name == col)
        .map(|c| c.kind)
        .unwrap_or(FieldKind::Text)
}

fn coerce_value(field: FieldOrSystem<'_>, op: Op, col: &str, raw: &str) -> Result<FilterValue, Error> {
    let kind = field.kind();
    match op {
        Op::IsNull => parse_bool(raw)
            .map(FilterValue::Null)
            .map_err(|reason| field_err(col, reason)),
        Op::Eq | Op::Ne => {
            if raw.eq_ignore_ascii_case("null") {
                return Ok(FilterValue::Bound(BoundValue::Null(kind)));
            }
            coerce_bound(kind, col, raw).map(FilterValue::Bound)
        }
        Op::Gt | Op::Gte | Op::Lt | Op::Lte => {
            if raw.eq_ignore_ascii_case("null") {
                return Err(field_err(col, "order operator cannot compare against null"));
            }
            coerce_bound(kind, col, raw).map(FilterValue::Bound)
        }
        Op::Contains | Op::StartsWith | Op::EndsWith | Op::ContainsI => {
            let escaped = escape_like(raw);
            let wrapped = wrap_like(op, escaped);
            Ok(FilterValue::Bound(BoundValue::Str(wrapped)))
        }
        // Set ops are handled directly in `parse`, not here.
        Op::In | Op::NotIn => {
            Err(field_err(col, "internal: set op routed through coerce_value"))
        }
        // Unreachable today: every Op variant above is handled. The wildcard
        // exists because `Op` is `#[non_exhaustive]` so a future variant
        // compiles silently until both `map_op` and this match get updated.
        _ => Err(field_err(col, "unsupported operator")),
    }
}

fn coerce_bound(kind: FieldKind, col: &str, raw: &str) -> Result<BoundValue, Error> {
    let v = match kind {
        FieldKind::String | FieldKind::Text => BoundValue::Str(raw.to_string()),
        FieldKind::Integer => raw
            .parse::<i64>()
            .map(BoundValue::I64)
            .map_err(|_| field_err(col, "expected integer"))?,
        FieldKind::Float => raw
            .parse::<f64>()
            .map(BoundValue::F64)
            .map_err(|_| field_err(col, "expected number"))?,
        FieldKind::Boolean => parse_bool(raw)
            .map(BoundValue::Bool)
            .map_err(|reason| field_err(col, reason))?,
        FieldKind::Datetime => chrono::DateTime::parse_from_rfc3339(raw)
            .map(|t| BoundValue::DateTime(t.with_timezone(&chrono::Utc)))
            .map_err(|_| field_err(col, "expected RFC3339 datetime"))?,
        FieldKind::Uuid => {
            uuid::Uuid::parse_str(raw).map_err(|_| field_err(col, "expected UUID"))?;
            BoundValue::Str(raw.to_string())
        }
        _ => return Err(field_err(col, "unsupported kind for filter")),
    };
    Ok(v)
}

fn parse_bool(raw: &str) -> Result<bool, String> {
    match raw.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("expected `true` or `false`".into()),
    }
}

fn field_err(col: &str, reason: impl Into<String>) -> Error {
    Error::Validation(ValidationErrors::field(col, reason))
}

/// Escape LIKE metacharacters in user input. Order matters: backslash first
/// so we don't double-escape our own substitutions.
fn escape_like(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn wrap_like(op: Op, escaped: String) -> String {
    match op {
        Op::Contains | Op::ContainsI => format!("%{escaped}%"),
        Op::StartsWith => format!("{escaped}%"),
        Op::EndsWith => format!("%{escaped}"),
        // Unreachable: caller filters by op group.
        _ => escaped,
    }
}

#[cfg(test)]
mod tokenize_tests {
    use super::*;

    #[test]
    fn flat_leaf() {
        let segs = tokenize_key("filters[title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn flat_leaf_with_in_index() {
        let segs = tokenize_key("filters[views][$in][0]").unwrap();
        assert_eq!(segs, vec![
            Segment::Name("views".into()),
            Segment::Op("$in".into()),
            Segment::Index(0),
        ]);
    }

    #[test]
    fn or_group_index_then_leaf() {
        let segs = tokenize_key("filters[$or][0][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$or".into()),
            Segment::Index(0),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn not_wraps_leaf() {
        let segs = tokenize_key("filters[$not][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$not".into()),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn nested_or_in_or() {
        let segs = tokenize_key("filters[$or][0][$or][1][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$or".into()),
            Segment::Index(0),
            Segment::Combinator("$or".into()),
            Segment::Index(1),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn missing_filters_prefix_rejected() {
        assert!(tokenize_key("title[$eq]").is_err());
    }

    #[test]
    fn unbalanced_brackets_rejected() {
        assert!(tokenize_key("filters[title][$eq").is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::{Field, FieldKind};
    use serde_json::json;
    use uuid::Uuid;

    fn ct() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: true,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "views".into(),
                    kind: FieldKind::Integer,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "published".into(),
                    kind: FieldKind::Boolean,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn leaves(f: Filter) -> Vec<Condition> {
        let Filter::All(xs) = f else { panic!("expected All") };
        xs.into_iter()
            .map(|x| match x {
                Filter::Leaf(c) => c,
                other => panic!("expected Leaf, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn empty_returns_none() {
        let f = parse("", &ct()).unwrap();
        assert!(matches!(f, Filter::None));
    }

    #[test]
    fn ignores_non_filter_params() {
        let f = parse("page=1&pageSize=25&sort=created_at:desc", &ct()).unwrap();
        assert!(matches!(f, Filter::None));
    }

    #[test]
    fn single_eq_string() {
        let f = parse("filters[title][$eq]=hi", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds.len(), 1);
        assert_eq!(conds[0].column, "title");
        assert_eq!(conds[0].op, Op::Eq);
    }

    #[test]
    fn integer_coerces() {
        let f = parse("filters[views][$ne]=7", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::I64(n)) => assert_eq!(*n, 7),
            other => panic!("expected I64, got {other:?}"),
        }
    }

    #[test]
    fn bad_integer_rejected() {
        let err = parse("filters[views][$eq]=not-a-number", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn unknown_field_rejected() {
        let err = parse("filters[ghost][$eq]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn unknown_op_rejected() {
        let err = parse("filters[title][$bogus]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn malformed_bracket_rejected() {
        let err = parse("filters[title]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn null_true_and_false() {
        let f = parse("filters[views][$null]=true", &ct()).unwrap();
        let conds = leaves(f);
        assert!(matches!(conds[0].value, FilterValue::Null(true)));

        let f = parse("filters[views][$null]=false", &ct()).unwrap();
        let conds = leaves(f);
        assert!(matches!(conds[0].value, FilterValue::Null(false)));
    }

    #[test]
    fn null_value_invalid() {
        let err = parse("filters[views][$null]=maybe", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn eq_null_rewrites_to_typed_null() {
        let f = parse("filters[views][$eq]=null", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Null(k)) => assert_eq!(*k, FieldKind::Integer),
            other => panic!("expected typed Null, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_col_op_rejected() {
        let err = parse(
            "filters[views][$eq]=1&filters[views][$eq]=2",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn same_col_different_ops_allowed() {
        let f = parse("filters[views][$eq]=1&filters[views][$ne]=5", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds.len(), 2);
    }

    #[test]
    fn boolean_case_insensitive() {
        let f = parse("filters[published][$eq]=True", &ct()).unwrap();
        let conds = leaves(f);
        assert!(matches!(conds[0].value, FilterValue::Bound(BoundValue::Bool(true))));
    }

    #[test]
    fn system_column_filterable() {
        let f = parse("filters[id][$null]=false", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].column, "id");
    }

    #[test]
    fn escape_like_handles_metacharacters() {
        assert_eq!(escape_like("foo"), "foo");
        assert_eq!(escape_like("50%"), "50\\%");
        assert_eq!(escape_like("a_b"), "a\\_b");
        assert_eq!(escape_like("a\\b"), "a\\\\b");
        // Backslash-first ordering: input \% becomes \\\% not \\\\%.
        assert_eq!(escape_like("\\%"), "\\\\\\%");
    }

    #[test]
    fn wrap_like_per_op() {
        assert_eq!(wrap_like(Op::Contains, "foo".into()), "%foo%");
        assert_eq!(wrap_like(Op::ContainsI, "foo".into()), "%foo%");
        assert_eq!(wrap_like(Op::StartsWith, "foo".into()), "foo%");
        assert_eq!(wrap_like(Op::EndsWith, "foo".into()), "%foo");
    }

    #[test]
    fn gt_on_string_rejected() {
        let err = parse("filters[title][$gt]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn contains_on_integer_rejected() {
        let err = parse("filters[views][$contains]=7", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn gt_integer_parses() {
        let f = parse("filters[views][$gt]=10", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].op, Op::Gt);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::I64(n)) => assert_eq!(*n, 10),
            other => panic!("expected I64, got {other:?}"),
        }
    }

    #[test]
    fn in_two_values_collects_into_list() {
        let f = parse("filters[views][$in][0]=1&filters[views][$in][1]=2", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds.len(), 1);
        assert_eq!(conds[0].op, Op::In);
        match &conds[0].value {
            FilterValue::List(vs) => {
                assert_eq!(vs.len(), 2);
                assert!(matches!(vs[0], BoundValue::I64(1)));
                assert!(matches!(vs[1], BoundValue::I64(2)));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn in_missing_index_rejected() {
        let err = parse("filters[views][$in]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn non_set_op_with_index_rejected() {
        let err = parse("filters[views][$eq][0]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_duplicate_index_rejected() {
        let err = parse(
            "filters[views][$in][0]=1&filters[views][$in][0]=2",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_null_entry_rejected() {
        let err = parse("filters[views][$in][0]=null", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_over_cap_rejected() {
        let mut q = String::new();
        for i in 0..=100 {
            if !q.is_empty() {
                q.push('&');
            }
            q.push_str(&format!("filters[views][$in][{i}]={i}"));
        }
        let err = parse(&q, &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn contains_escapes_and_wraps() {
        let f = parse("filters[title][$contains]=50%25", &ct()).unwrap();
        // `%25` URL-decodes to `%`, which then escapes to `\%`, then wraps to `%50\%%`.
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%50\\%%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn starts_with_wraps_one_side() {
        let f = parse("filters[title][$startsWith]=foo", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "foo%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn ends_with_wraps_one_side() {
        let f = parse("filters[title][$endsWith]=foo", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%foo"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn containsi_op_variant() {
        let f = parse("filters[title][$containsi]=FOO", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].op, Op::ContainsI);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%FOO%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn gte_on_datetime_rfc3339() {
        let f = parse("filters[created_at][$gte]=2026-01-01T00:00:00Z", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].op, Op::Gte);
        assert!(matches!(conds[0].value, FilterValue::Bound(BoundValue::DateTime(_))));
    }
}
