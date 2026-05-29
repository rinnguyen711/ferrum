//! Strapi-style `?filters[col][$op]=value` parser. Produces a `rustapi_sql::Filter`
//! ready for the SQL builder. v1 supports `$eq`, `$ne`, `$null` with implicit
//! AND across params.

use rustapi_core::{is_system_column, BoundValue, ContentType, Error, Field, FieldKind, ValidationErrors, SYSTEM_COLUMNS};
use rustapi_sql::{Condition, Filter, FilterValue, Op};
use std::collections::HashSet;
use std::sync::OnceLock;
use url::form_urlencoded;

/// Parse a raw query string into a `Filter`. Non-filter params are ignored.
/// Returns `Filter::None` if no filter params are present.
pub fn parse(raw_query: &str, ct: &ContentType) -> Result<Filter, Error> {
    let mut seen: HashSet<(String, Op)> = HashSet::new();
    let mut conds: Vec<Condition> = Vec::new();

    for (k, v) in form_urlencoded::parse(raw_query.as_bytes()) {
        if !k.starts_with("filters[") {
            continue;
        }
        let (col, op_str) = parse_key(&k)?;
        let op = match op_str.as_str() {
            "$eq" => Op::Eq,
            "$ne" => Op::Ne,
            "$null" => Op::IsNull,
            other => {
                return Err(Error::Validation(ValidationErrors::field(
                    col,
                    format!("unknown operator `{other}`"),
                )));
            }
        };

        let field = field_for(ct, &col)?;
        if !seen.insert((col.clone(), op)) {
            return Err(Error::Validation(ValidationErrors::field(
                col,
                "duplicate filter operator on column",
            )));
        }

        let kind = field.kind();
        let value = coerce_value(field, op, &col, &v)?;
        conds.push(Condition::new(col, kind, op, value));
    }

    if conds.is_empty() {
        Ok(Filter::None)
    } else {
        Ok(Filter::All(conds))
    }
}

fn parse_key(k: &str) -> Result<(String, String), Error> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^filters\[(?P<col>[^\[\]]+)\]\[(?P<op>\$[a-z]+)\]$").unwrap()
    });
    let caps = re.captures(k).ok_or_else(|| {
        Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}`"
        )))
    })?;
    Ok((caps["col"].to_string(), caps["op"].to_string()))
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
        // Unreachable today: `parse` only constructs Eq / Ne / IsNull from the
        // closed `$eq` / `$ne` / `$null` mapping. The wildcard exists because
        // `Op` is `#[non_exhaustive]` (cross-crate) so the match must be open;
        // a future variant added in the sql crate compiles silently here until
        // both the `$op` mapping above AND this `match` get updated.
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
        let Filter::All(conds) = f else { panic!("expected All") };
        assert_eq!(conds.len(), 1);
        assert_eq!(conds[0].column, "title");
        assert_eq!(conds[0].op, Op::Eq);
    }

    #[test]
    fn integer_coerces() {
        let f = parse("filters[views][$ne]=7", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
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
        let Filter::All(conds) = f else { panic!() };
        assert!(matches!(conds[0].value, FilterValue::Null(true)));

        let f = parse("filters[views][$null]=false", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
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
        let Filter::All(conds) = f else { panic!() };
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
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds.len(), 2);
    }

    #[test]
    fn boolean_case_insensitive() {
        let f = parse("filters[published][$eq]=True", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert!(matches!(conds[0].value, FilterValue::Bound(BoundValue::Bool(true))));
    }

    #[test]
    fn system_column_filterable() {
        let f = parse("filters[id][$null]=false", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds[0].column, "id");
    }
}
