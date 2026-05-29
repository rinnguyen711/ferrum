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
            Error::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "missing or invalid API key".to_string(), None),
            Error::NotFound => (StatusCode::NOT_FOUND, "not_found", "resource not found".to_string(), None),
            Error::Validation(v) => {
                let msg = v.message.clone().unwrap_or_else(|| "validation failed".into());
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
                let details = if detail_obj.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(detail_obj))
                };
                (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed", msg, details)
            }
            Error::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg, None),
            Error::Unsupported(msg) => (StatusCode::BAD_REQUEST, "unsupported", msg, None),
            Error::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error".into(), None)
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
}
