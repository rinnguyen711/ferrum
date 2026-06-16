//! Opaque base64 cursor token for keyset pagination. Encodes the
//! `(sort_col, dir, last_sort_value, last_id)` position. Clients treat the
//! token as a black box; we keep it opaque so internals can change freely.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rustapi_core::{BoundValue, FieldKind};
use rustapi_sql::{Sort, SortDir};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub enum CursorError {
    /// Token is not valid base64 / JSON, or fields are missing/ill-typed.
    Malformed,
    /// Token's sort column or direction does not match the request's sort.
    SortMismatch,
}

#[derive(Serialize, Deserialize)]
struct Payload {
    /// sort column
    c: String,
    /// direction: "asc" | "desc"
    d: String,
    /// last sort value as JSON
    v: serde_json::Value,
    /// last row id
    i: String,
}

fn dir_str(dir: SortDir) -> &'static str {
    match dir {
        SortDir::Asc => "asc",
        SortDir::Desc => "desc",
    }
}

/// Encode the cursor position into an opaque token.
pub fn encode(sort: &Sort, last_value: &BoundValue, last_id: Uuid) -> String {
    let v = bound_to_json(last_value);
    let payload = Payload {
        c: sort.column.clone(),
        d: dir_str(sort.dir).to_string(),
        v,
        i: last_id.to_string(),
    };
    let bytes = serde_json::to_vec(&payload).expect("payload serializes");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decode + validate a token against the request's current `sort` and the sort
/// column's `kind`. Returns `(sort_value_bind, last_id)` for the seek query.
pub fn decode(
    token: &str,
    sort: &Sort,
    kind: FieldKind,
) -> Result<(BoundValue, Uuid), CursorError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| CursorError::Malformed)?;
    let payload: Payload = serde_json::from_slice(&bytes).map_err(|_| CursorError::Malformed)?;

    if payload.c != sort.column || payload.d != dir_str(sort.dir) {
        return Err(CursorError::SortMismatch);
    }

    let id = Uuid::parse_str(&payload.i).map_err(|_| CursorError::Malformed)?;
    let val = json_to_bound(&payload.v, kind).ok_or(CursorError::Malformed)?;
    Ok((val, id))
}

/// Serialize a bound sort value to JSON for the token. Sort values come from a
/// stored column, so only the scalar kinds appear here.
fn bound_to_json(v: &BoundValue) -> serde_json::Value {
    match v {
        BoundValue::Str(s) => serde_json::Value::String(s.clone()),
        BoundValue::I64(n) => serde_json::json!(n),
        BoundValue::F64(f) => serde_json::json!(f),
        BoundValue::Bool(b) => serde_json::json!(b),
        BoundValue::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
        BoundValue::Uuid(u) => serde_json::Value::String(u.to_string()),
        BoundValue::Null(_) | BoundValue::Json(_) => serde_json::Value::Null,
    }
}

/// Rebuild a typed `BoundValue` from the token JSON for the sort column's kind.
/// String/Text/Datetime/Uuid all bind as `Str` (matches how the seek query
/// casts placeholders), Integer as `I64`, etc. Returns `None` on type mismatch.
fn json_to_bound(v: &serde_json::Value, kind: FieldKind) -> Option<BoundValue> {
    match kind {
        FieldKind::String | FieldKind::Text | FieldKind::Datetime | FieldKind::Uuid => {
            v.as_str().map(|s| BoundValue::Str(s.to_string()))
        }
        FieldKind::Integer => v.as_i64().map(BoundValue::I64),
        FieldKind::Float => v.as_f64().map(BoundValue::F64),
        FieldKind::Boolean => v.as_bool().map(BoundValue::Bool),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sort_desc(col: &str) -> Sort {
        Sort {
            column: col.into(),
            dir: SortDir::Desc,
        }
    }

    #[test]
    fn round_trip_datetime() {
        let sort = sort_desc("created_at");
        let id = Uuid::new_v4();
        let val = BoundValue::Str("2024-01-01T00:00:00Z".into());
        let tok = encode(&sort, &val, id);
        let (decoded_val, decoded_id) = decode(&tok, &sort, FieldKind::Datetime).unwrap();
        assert_eq!(decoded_id, id);
        assert_eq!(decoded_val, BoundValue::Str("2024-01-01T00:00:00Z".into()));
    }

    #[test]
    fn round_trip_integer() {
        let sort = sort_desc("views");
        let id = Uuid::new_v4();
        let val = BoundValue::I64(500);
        let tok = encode(&sort, &val, id);
        let (decoded_val, _) = decode(&tok, &sort, FieldKind::Integer).unwrap();
        assert_eq!(decoded_val, BoundValue::I64(500));
    }

    #[test]
    fn garbage_token_is_malformed() {
        let sort = sort_desc("created_at");
        let err = decode("!!!not-base64!!!", &sort, FieldKind::Datetime).unwrap_err();
        assert_eq!(err, CursorError::Malformed);
    }

    #[test]
    fn sort_mismatch_rejected() {
        let enc_sort = sort_desc("created_at");
        let tok = encode(&enc_sort, &BoundValue::Str("2024".into()), Uuid::new_v4());
        let req_sort = sort_desc("title");
        let err = decode(&tok, &req_sort, FieldKind::String).unwrap_err();
        assert_eq!(err, CursorError::SortMismatch);
    }

    #[test]
    fn dir_mismatch_rejected() {
        let enc_sort = sort_desc("created_at");
        let tok = encode(&enc_sort, &BoundValue::Str("2024".into()), Uuid::new_v4());
        let req_sort = Sort {
            column: "created_at".into(),
            dir: SortDir::Asc,
        };
        let err = decode(&tok, &req_sort, FieldKind::Datetime).unwrap_err();
        assert_eq!(err, CursorError::SortMismatch);
    }

    #[test]
    fn bad_uuid_is_malformed() {
        let payload = json!({"c":"created_at","d":"desc","v":"2024","i":"not-a-uuid"});
        let raw = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let sort = sort_desc("created_at");
        let err = decode(&raw, &sort, FieldKind::String).unwrap_err();
        assert_eq!(err, CursorError::Malformed);
    }
}
