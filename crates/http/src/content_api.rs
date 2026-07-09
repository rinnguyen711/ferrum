//! Public content service API.
//!
//! Call the content engine (create / get / update / delete an entry of any
//! content type) from outside the HTTP layer — e.g. custom routers injected via
//! `build_router(state, extra)`. Every operation runs the same pipeline as the
//! REST handlers: authorization, write-hooks, validation, relation/media checks,
//! event emission, and audit logging.
//!
//! `Principal` and `RequestContext` come from `ferrum_core`; a custom axum
//! handler extracts them from request extensions (the auth + reqctx middleware
//! inject them on the protected router). For non-HTTP callers,
//! `RequestContext::default()` is acceptable.
//!
//! `list_entries` is intentionally not exposed yet (its params are HTTP-shaped).

pub use crate::routes::content::{create_entry, delete_entry, get_entry, update_entry};
