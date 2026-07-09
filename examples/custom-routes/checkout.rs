//! EXAMPLE — custom business endpoint: `POST /api/checkout`
//! ============================================================================
//! Illustrative reference, NOT compiled into the build. Shows how a dev adds a
//! custom route that reads content-type entries (products) and creates another
//! (an order) using the PUBLIC content service API (`ferrum_http::content_api`).
//!
//! Wire it up in `crates/bin/src/main.rs`:
//!
//! ```ignore
//! let custom = axum::Router::new().route("/api/checkout", axum::routing::post(checkout));
//! let mut app = build_router(state, vec![custom]);   // <- was vec![]
//! ```
//!
//! The router merges into the PROTECTED tree: behind `require_auth` (caller
//! needs a valid Bearer token) and sharing `AppState`. The auth + reqctx
//! middleware inject `Principal` and `RequestContext` as request extensions, so
//! the handler extracts them and passes them straight to the content service.
//! A duplicate path+method panics at startup, so collisions surface immediately.
//!
//! Every `content_api` call runs the SAME pipeline as the REST handlers:
//! authorization, write-hooks, field + component validation, relation/media
//! checks, event emission, and audit logging. An order created here is identical
//! to one created via `POST /api/order` — same validation, same webhooks fire,
//! same audit entry. No hand-written SQL, no bypassed side-effects.
//!
//! Note on the error type: handler results use `ferrum_http::ApiError` (not
//! `ferrum_core::Error` directly) because `ApiError` is what implements axum's
//! `IntoResponse`. `ApiError: From<Error>`, so `?` on a `content_api` call
//! converts cleanly and the response gets the standard `{"error":{...}}` shape.

use axum::{extract::State, Extension, Json};
use ferrum_core::{Principal, RequestContext};
use ferrum_http::{content_api, ApiError, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CheckoutReq {
    pub customer_id: Uuid,
    pub items: Vec<CheckoutItem>,
}

#[derive(Deserialize)]
pub struct CheckoutItem {
    pub product_id: Uuid,
    pub quantity: i64,
}

#[derive(Serialize)]
pub struct CheckoutResp {
    pub order_number: String,
    pub total: f64,
    pub order_id: String,
}

/// POST /api/checkout — price the cart from live product entries, then create
/// an order through the content service.
pub async fn checkout(
    State(st): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(req_ctx): Extension<RequestContext>,
    Json(req): Json<CheckoutReq>,
) -> Result<Json<CheckoutResp>, ApiError> {
    // ---- 1. read each product through the real pipeline (authz + populate) --
    let mut lines: Vec<Value> = Vec::new();
    let mut total = 0.0_f64;

    for item in &req.items {
        // get_entry: authz-checked, components/relations populated, row -> JSON.
        let product =
            content_api::get_entry(&st, &principal, "product", item.product_id, None).await?;

        // `price` is a shared.price component: { amount, currency, compare_at }.
        let unit_price = product
            .get("price")
            .and_then(|p| p.get("amount"))
            .and_then(|a| a.as_f64())
            .unwrap_or(0.0);

        total += unit_price * item.quantity as f64;
        lines.push(json!({
            "product_ref": product.get("title").cloned().unwrap_or(Value::Null),
            "sku": product.get("sku").cloned().unwrap_or(Value::Null),
            "quantity": item.quantity,
            "unit_price": unit_price,
        }));
    }

    // ---- 2. create the order through the real pipeline ----------------------
    // validation + write hooks + event emission + audit all happen inside.
    let mut body = Map::new();
    body.insert(
        "order_number".into(),
        json!(format!("ORD-{}", &Uuid::new_v4().to_string()[..8])),
    );
    body.insert("customer".into(), json!(req.customer_id)); // relation by id
    body.insert("items".into(), Value::Array(lines));
    body.insert("total".into(), json!(total));
    body.insert("status".into(), json!("pending"));

    let order = content_api::create_entry(&st, &principal, &req_ctx, "order", body).await?;

    Ok(Json(CheckoutResp {
        order_number: order
            .get("order_number")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        total,
        order_id: order
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }))
}
