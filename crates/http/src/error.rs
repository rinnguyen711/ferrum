//! Map `rustapi_core::Error` into HTTP responses with the v1 JSON shape.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rustapi_core::Error;
use serde_json::json;

pub struct ApiError(pub Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self.0 {
            Error::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid credentials".to_string(),
                None,
            ),
            Error::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "insufficient permissions".to_string(),
                None,
            ),
            Error::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "resource not found".to_string(),
                None,
            ),
            Error::Validation(v) => {
                let msg = v
                    .message
                    .clone()
                    .unwrap_or_else(|| "validation failed".into());
                let mut detail_obj = serde_json::Map::new();
                if !v.fields.is_empty() {
                    detail_obj.insert(
                        "fields".into(),
                        serde_json::to_value(&v.fields).unwrap_or(serde_json::Value::Null),
                    );
                }
                if let Some(db) = &v.db {
                    detail_obj.insert(
                        "db".into(),
                        serde_json::to_value(db).unwrap_or(serde_json::Value::Null),
                    );
                }
                if !v.missing_ids.is_empty() {
                    detail_obj.insert(
                        "missing_ids".into(),
                        serde_json::to_value(&v.missing_ids).unwrap_or(serde_json::Value::Null),
                    );
                }
                let details = if detail_obj.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(detail_obj))
                };
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "validation_failed",
                    msg,
                    details,
                )
            }
            Error::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg, None),
            Error::Unsupported(msg) => (StatusCode::BAD_REQUEST, "unsupported", msg, None),
            Error::RelationFkViolation { constraint } => {
                let details = constraint.map(|c| json!({ "constraint": c }));
                (
                    StatusCode::CONFLICT,
                    "relation_fk_violation",
                    "relation FK violation".into(),
                    details,
                )
            }
            Error::EnumValueNotAllowed {
                field,
                value,
                allowed,
            } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "enum_value_not_allowed",
                format!("value `{value}` not allowed for `{field}`"),
                Some(json!({"field": field, "value": value, "allowed": allowed})),
            ),
            Error::BadEmail => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "bad_email",
                "invalid email".into(),
                None,
            ),
            Error::BadUrl => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "bad_url",
                "invalid URL".into(),
                None,
            ),
            Error::BadSlug => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "bad_slug",
                "invalid slug".into(),
                None,
            ),
            Error::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".into(),
                    None,
                )
            }
        };
        let body = json!({
            "error": {
                "code": code,
                "message": message,
                "details": details,
            }
        });
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn unauthorized_shape() {
        let resp = ApiError(Error::Unauthorized).into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn forbidden_is_403() {
        let resp = ApiError(Error::Forbidden).into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "forbidden");
    }

    #[tokio::test]
    async fn not_found_shape() {
        let resp = ApiError(Error::NotFound).into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn validation_includes_fields() {
        let v = rustapi_core::ValidationErrors::field("title", "required");
        let resp = ApiError(Error::Validation(v)).into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "validation_failed");
        assert_eq!(body["error"]["details"]["fields"][0]["field"], "title");
    }

    #[tokio::test]
    async fn relation_fk_violation_is_409() {
        let resp = ApiError(Error::RelationFkViolation {
            constraint: Some("ct_post_author_id_fkey".into()),
        })
        .into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "relation_fk_violation");
        assert_eq!(
            body["error"]["details"]["constraint"],
            "ct_post_author_id_fkey"
        );
    }

    #[tokio::test]
    async fn validation_includes_missing_ids() {
        let v = rustapi_core::ValidationErrors::relation_target_missing(
            "author",
            vec!["00000000-0000-0000-0000-000000000001".into()],
        );
        let resp = ApiError(Error::Validation(v)).into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "validation_failed");
        assert_eq!(
            body["error"]["details"]["missing_ids"][0],
            "00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(body["error"]["details"]["fields"][0]["field"], "author");
    }

    #[tokio::test]
    async fn validation_includes_db_info() {
        let v = rustapi_core::ValidationErrors::db("23502", "null value in column \"x\"");
        let resp = ApiError(Error::Validation(v)).into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["details"]["db"]["code"], "23502");
        assert!(body["error"]["details"]["db"]["message"]
            .as_str()
            .unwrap()
            .contains("null value"));
    }

    #[tokio::test]
    async fn enum_value_not_allowed_is_422() {
        let resp = ApiError(Error::EnumValueNotAllowed {
            field: "status".into(),
            value: "bad".into(),
            allowed: vec!["draft".into(), "published".into()],
        })
        .into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "enum_value_not_allowed");
        assert_eq!(body["error"]["details"]["field"], "status");
        assert_eq!(body["error"]["details"]["value"], "bad");
        assert_eq!(body["error"]["details"]["allowed"][0], "draft");
    }

    #[tokio::test]
    async fn bad_email_is_422() {
        let resp = ApiError(Error::BadEmail).into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "bad_email");
    }

    #[tokio::test]
    async fn bad_url_is_422() {
        let resp = ApiError(Error::BadUrl).into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn bad_slug_is_422() {
        let resp = ApiError(Error::BadSlug).into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
