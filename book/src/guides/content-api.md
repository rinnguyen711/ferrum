# Call the content API from Rust

The content API lets your own Rust code create, read, update, and delete entries
of any content type — the same operations the [REST API](../reference/rest-api.md)
exposes, but called in-process. Use it from a [custom route](#wire-up-a-custom-route)
to build business endpoints (a checkout, a bulk importer, a webhook receiver)
that work with content the way the rest of the server does.

Every call runs the **same pipeline** as the REST handlers: authorization,
[write hooks](write-hooks.md), field and component validation, relation and media
checks, event emission (so [webhooks](webhooks.md) fire), and audit logging. An
entry you create through the content API is identical to one created via
`POST /api/<type>` — no hand-written SQL, no bypassed side effects.

## The functions

The API lives in `ferrum_http::content_api` as four async functions:

```rust
use ferrum_http::content_api;

// Read one entry by id. `populate` mirrors the ?populate= query param.
get_entry(state, principal, ct_name, id, populate) -> Result<Value, Error>

// Create. `body` is the entry fields as a JSON object.
create_entry(state, principal, req_ctx, ct_name, body) -> Result<Value, Error>

// Full-replace update (PUT semantics): absent non-required fields get nulled.
update_entry(state, principal, req_ctx, ct_name, id, body) -> Result<Value, Error>

// Delete by id.
delete_entry(state, principal, req_ctx, ct_name, id) -> Result<(), Error>
```

The shared arguments:

- `state: &AppState` — the server state, available in every handler via axum's
  `State` extractor.
- `principal: &Principal` — who is making the request. Authorization is checked
  against this; a principal lacking `content:read` on the type gets a `403`.
- `req_ctx: &RequestContext` — request metadata used for the audit trail. The
  write operations take it; `get_entry` does not.
- `ct_name: &str` — the content type, e.g. `"article"` or `"order"`.
- `body: Map<String, Value>` — entry fields. Set a relation by passing the
  target id; set a component by passing its nested object.

Listing entries is not exposed yet — its parameters are HTTP-shaped. Call the
REST `GET /api/<type>` endpoint for now.

## Wire up a custom route

Pass your router to `build_router` as the `extra` argument. It merges into the
**protected** tree — behind `require_auth`, sharing `AppState`:

```rust
use ferrum_http::build_router;

let custom = axum::Router::new()
    .route("/api/checkout", axum::routing::post(checkout));

let mut app = build_router(state, vec![custom]);   // was vec![]
```

A duplicate path + method panics at startup, so collisions surface immediately.

## Extract the principal in a handler

The auth and reqctx middleware inject `Principal` and `RequestContext` as request
extensions on the protected tree. A handler pulls them out with axum's
`Extension` extractor and passes them straight through:

```rust
use axum::{extract::State, Extension, Json};
use ferrum_core::{Principal, RequestContext};
use ferrum_http::{content_api, ApiError, AppState};
use serde_json::{json, Map};
use uuid::Uuid;

pub async fn checkout(
    State(st): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(req_ctx): Extension<RequestContext>,
    Json(req): Json<CheckoutReq>,
) -> Result<Json<CheckoutResp>, ApiError> {
    // Read a product through the real pipeline (authz + populate).
    let product =
        content_api::get_entry(&st, &principal, "product", req.product_id, None).await?;

    // Build an order body and create it — validation, hooks, events, audit
    // all happen inside create_entry.
    let mut body = Map::new();
    body.insert(
        "order_number".into(),
        json!(format!("ORD-{}", &Uuid::new_v4().to_string()[..8])),
    );
    body.insert("customer".into(), json!(req.customer_id)); // relation by id
    body.insert("total".into(), json!(product["price"]["amount"].as_f64()));

    let order = content_api::create_entry(&st, &principal, &req_ctx, "order", body).await?;

    Ok(Json(CheckoutResp { order_id: order["id"].as_str().unwrap_or("").into() }))
}
```

A full, annotated version of this handler is in
`examples/custom-routes/checkout.rs`.

## Error handling

Handler results use `ferrum_http::ApiError`, not `ferrum_core::Error` directly,
because `ApiError` is what implements axum's `IntoResponse`. `ApiError: From<Error>`,
so `?` on a `content_api` call converts cleanly and the response gets the standard
`{"error": {...}}` shape. A missing entry becomes `404`, a failed authz check
`403`, a validation failure `422`.

## Calling from outside HTTP

The functions don't require a live request. A background job or CLI subcommand
can call them too — construct or hold an `AppState`, supply a `Principal`, and
pass `RequestContext::default()` where a write needs one.

## Where to go next

- [Write hooks](write-hooks.md) — run code on the write path that these calls trigger.
- [Webhooks](webhooks.md) — the events these calls emit can drive outbound webhooks.
- [REST API](../reference/rest-api.md) — the HTTP surface backed by the same functions.
