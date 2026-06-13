//! Real resolver factories for the dynamic GraphQL schema. `build.rs` wires
//! these onto every field. Output-object / envelope / Meta / Media fields use
//! [`json_field_resolver`], which threads the parent JSON object down to its
//! children. Root query/mutation fields delegate to the shared `content::`
//! CRUD functions (the same code the REST handlers call), so authz, validation,
//! and storage behavior are identical across both surfaces.
//!
//! AppState + Principal are injected per-request via `request.data(..)` (Task 7),
//! and read back here through `ctx.data::<T>()` (ResolverContext derefs to the
//! async-graphql `Context`).
//!
//! Populate: list/get derive which first-level relation/media fields the client
//! selected via the selection set (`selected_field_names` + `populate_arg`) and
//! pass them as the batched `populate` arg to `content::list_entries`/`get_entry`,
//! which embeds the related object(s) into each row's JSON before child resolvers
//! run. Deeper sub-relations are not populated and resolve to GraphQL `null`
//! (`json_field_resolver` returns `None` for absent keys).

use async_graphql::dynamic::{FieldFuture, FieldValue, ResolverContext};
use async_graphql::{Error as GqlError, ErrorExtensions, Value as GqlValue};
use rustapi_core::{ContentType, Error, FieldKind, Principal};
use serde_json::{Map, Value as JsonValue};
use uuid::Uuid;

use crate::query::ListParams;
use crate::routes::content;
use crate::state::AppState;

// --- context helpers -------------------------------------------------------

fn app_state(ctx: &ResolverContext<'_>) -> Result<AppState, GqlError> {
    ctx.data::<AppState>()
        .cloned()
        .map_err(|_| GqlError::new("AppState missing from GraphQL context"))
}

fn principal(ctx: &ResolverContext<'_>) -> Result<Principal, GqlError> {
    ctx.data::<Principal>()
        .cloned()
        .map_err(|_| GqlError::new("Principal missing from GraphQL context"))
}

// --- error mapping ---------------------------------------------------------

/// Map a `core::Error` to a GraphQL error carrying `extensions.code`. Task 9
/// asserts on `errors[].extensions.code`.
fn gql_err(e: Error) -> GqlError {
    let code = match &e {
        Error::NotFound => "NOT_FOUND",
        Error::Validation(_)
        | Error::Unsupported(_)
        | Error::BadEmail
        | Error::BadUrl
        | Error::BadSlug
        | Error::EnumValueNotAllowed { .. } => "BAD_USER_INPUT",
        Error::Forbidden => "FORBIDDEN",
        Error::Unauthorized => "UNAUTHORIZED",
        Error::Conflict(_) | Error::RelationFkViolation { .. } => "CONFLICT",
        _ => "INTERNAL",
    };
    GqlError::new(e.to_string()).extend_with(|_, ext| ext.set("code", code))
}

// --- value conversion ------------------------------------------------------

/// serde_json::Value -> async_graphql::Value. Falls back to Null on the
/// (practically unreachable) conversion failure.
fn json_to_gql(v: JsonValue) -> GqlValue {
    GqlValue::from_json(v).unwrap_or(GqlValue::Null)
}

// --- field (child) resolver ------------------------------------------------

/// Pull `key` out of the parent object Value and hand it down as the child's
/// own parent value, so nested object fields keep reading from the same JSON
/// tree. Returns `None` (GraphQL null) when the key is absent or null.
pub fn json_field_resolver(key: &str) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    let key = key.to_string();
    move |ctx| {
        let key = key.clone();
        FieldFuture::new(async move {
            let obj = match ctx.parent_value.as_value() {
                Some(GqlValue::Object(m)) => m,
                _ => return Ok(None),
            };
            match obj.get(key.as_str()) {
                Some(v) if v != &GqlValue::Null => Ok(Some(FieldValue::value(v.clone()))),
                _ => Ok(None),
            }
        })
    }
}

// --- arg helpers -----------------------------------------------------------

/// Read an optional u32 arg (page / pageSize).
fn opt_u32(ctx: &ResolverContext<'_>, name: &str) -> Option<u32> {
    ctx.args
        .get(name)
        .and_then(|v| v.u64().ok())
        .map(|n| n as u32)
}

/// Read an optional String arg (sort).
fn opt_string(ctx: &ResolverContext<'_>, name: &str) -> Option<String> {
    ctx.args
        .get(name)
        .and_then(|v| v.string().ok())
        .map(|s| s.to_string())
}

/// Read the required `id` arg and parse it as a Uuid.
fn id_arg(ctx: &ResolverContext<'_>) -> Result<Uuid, GqlError> {
    let raw = ctx
        .args
        .get("id")
        .ok_or_else(|| {
            gql_err(Error::Validation(rustapi_core::ValidationErrors::single(
                "missing `id` argument",
            )))
        })?
        .string()
        .map_err(|_| {
            gql_err(Error::Validation(rustapi_core::ValidationErrors::single(
                "`id` must be a string",
            )))
        })?
        .to_string();
    Uuid::parse_str(&raw).map_err(|_| {
        gql_err(Error::Validation(rustapi_core::ValidationErrors::single(
            "`id` is not a valid UUID",
        )))
    })
}

/// Read the `data` input arg as a JSON object map.
fn data_arg(ctx: &ResolverContext<'_>) -> Result<Map<String, JsonValue>, GqlError> {
    let val: JsonValue = ctx
        .args
        .get("data")
        .ok_or_else(|| {
            gql_err(Error::Validation(rustapi_core::ValidationErrors::single(
                "missing `data` argument",
            )))
        })?
        .deserialize()
        .map_err(|_| {
            gql_err(Error::Validation(rustapi_core::ValidationErrors::single(
                "`data` is not a valid input object",
            )))
        })?;
    match val {
        JsonValue::Object(m) => Ok(m),
        _ => Err(gql_err(Error::Validation(
            rustapi_core::ValidationErrors::single("`data` must be an object"),
        ))),
    }
}

/// Convert the `filters` JSON-scalar arg into the raw query string that
/// `content::list_entries` parses. Shape:
/// `{"title":{"$containsi":"hi"}}` -> `filters[title][$containsi]=hi`.
fn filters_to_raw_query(v: JsonValue) -> String {
    // Percent-encode each dynamic component so a field/op/value containing `&`,
    // `=`, `%`, space, etc. can't corrupt the query string. The literal brackets
    // must survive un-encoded — `crate::filter::tokenize_key` scans for `[`/`]` —
    // so only the pieces *inside* the brackets and the value are encoded. This
    // mirrors what an HTTP client sends on the REST path, which `filter::parse`
    // already round-trips via `form_urlencoded::parse` (decodes both key + value).
    fn enc(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }
    let mut parts = Vec::new();
    if let JsonValue::Object(fields) = v {
        for (field, ops) in fields {
            if let JsonValue::Object(opmap) = ops {
                for (op, val) in opmap {
                    let val_s = match val {
                        JsonValue::String(s) => s,
                        other => other.to_string(),
                    };
                    parts.push(format!(
                        "filters[{}][{}]={}",
                        enc(&field),
                        enc(&op),
                        enc(&val_s)
                    ));
                }
            }
        }
    }
    parts.join("&")
}

// --- selection-set populate ------------------------------------------------

/// Sync: collect the names selected under the entry object. The `Lookahead`
/// borrows the resolver context, so the owned `Vec<String>` must be extracted
/// here (before the `FieldFuture`'s `async move`), then moved into the future.
fn selected_field_names(entry: async_graphql::Lookahead<'_>) -> Vec<String> {
    entry
        .selection_fields()
        .iter()
        .map(|sf| sf.name().to_string())
        .collect()
}

/// Filter selected names down to relation/media fields of `ct`, joined for the
/// REST-style `populate` arg. `None` when nothing to populate.
fn populate_arg(selected: &[String], ct: &ContentType) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for f in &ct.fields {
        if matches!(f.kind, FieldKind::Relation | FieldKind::Media)
            && selected.iter().any(|s| s == &f.name)
            && !out.contains(&f.name.as_str())
        {
            out.push(&f.name);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join(","))
    }
}

// --- root resolvers --------------------------------------------------------

/// Query list resolver: page / pageSize / sort / filters -> envelope.
pub fn list_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx| {
        let ct_name = ct_name.clone();
        let st = app_state(&ctx);
        let pr = principal(&ctx);
        let page = opt_u32(&ctx, "page");
        let page_size = opt_u32(&ctx, "pageSize");
        let sort = opt_string(&ctx, "sort");
        let raw_query = ctx
            .args
            .get("filters")
            .and_then(|v| v.deserialize::<JsonValue>().ok())
            .map(filters_to_raw_query)
            .unwrap_or_default();
        // Entry fields live under `data` in the list envelope. Collect the
        // selected names synchronously (the Lookahead borrows ctx); filter
        // against the ContentType inside the async block.
        let selected = selected_field_names(ctx.look_ahead().field("data"));
        FieldFuture::new(async move {
            let st = st?;
            let pr = pr?;
            let populate = st
                .schemas
                .registry()
                .get(&ct_name)
                .await
                .and_then(|ct| populate_arg(&selected, &ct));
            // `populate` is a separate arg to `content::list_entries`; the
            // `ListParams.populate` field stays `None`.
            let params = ListParams {
                page,
                page_size,
                sort,
                populate: None,
                status: None,
            };
            let env =
                content::list_entries(&st, &pr, &ct_name, params, populate.as_deref(), &raw_query)
                    .await
                    .map_err(gql_err)?;
            Ok(Some(FieldValue::value(json_to_gql(env))))
        })
    }
}

/// Query get-one resolver. `Error::NotFound` -> `null`; other errors propagate.
pub fn get_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx| {
        let ct_name = ct_name.clone();
        let st = app_state(&ctx);
        let pr = principal(&ctx);
        let id = id_arg(&ctx);
        // For get-one the entry fields are directly under the field (no `data`).
        let selected = selected_field_names(ctx.look_ahead());
        FieldFuture::new(async move {
            let st = st?;
            let pr = pr?;
            let id = id?;
            let populate = st
                .schemas
                .registry()
                .get(&ct_name)
                .await
                .and_then(|ct| populate_arg(&selected, &ct));
            match content::get_entry(&st, &pr, &ct_name, id, populate.as_deref()).await {
                Ok(entry) => Ok(Some(FieldValue::value(json_to_gql(entry)))),
                Err(Error::NotFound) => Ok(None),
                Err(e) => Err(gql_err(e)),
            }
        })
    }
}

/// Mutation create resolver.
pub fn create_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx| {
        let ct_name = ct_name.clone();
        let st = app_state(&ctx);
        let pr = principal(&ctx);
        let body = data_arg(&ctx);
        FieldFuture::new(async move {
            let st = st?;
            let pr = pr?;
            let body = body?;
            let entry = content::create_entry(&st, &pr, &ct_name, body)
                .await
                .map_err(gql_err)?;
            Ok(Some(FieldValue::value(json_to_gql(entry))))
        })
    }
}

/// Mutation update resolver.
pub fn update_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx| {
        let ct_name = ct_name.clone();
        let st = app_state(&ctx);
        let pr = principal(&ctx);
        let id = id_arg(&ctx);
        let body = data_arg(&ctx);
        FieldFuture::new(async move {
            let st = st?;
            let pr = pr?;
            let id = id?;
            let body = body?;
            let entry = content::update_entry(&st, &pr, &ct_name, id, body)
                .await
                .map_err(gql_err)?;
            Ok(Some(FieldValue::value(json_to_gql(entry))))
        })
    }
}

/// Mutation delete resolver. Returns `true` on success.
pub fn delete_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx| {
        let ct_name = ct_name.clone();
        let st = app_state(&ctx);
        let pr = principal(&ctx);
        let id = id_arg(&ctx);
        FieldFuture::new(async move {
            let st = st?;
            let pr = pr?;
            let id = id?;
            content::delete_entry(&st, &pr, &ct_name, id)
                .await
                .map_err(gql_err)?;
            Ok(Some(FieldValue::value(GqlValue::from(true))))
        })
    }
}
