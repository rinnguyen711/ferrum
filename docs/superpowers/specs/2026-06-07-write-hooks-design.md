# Write Hooks Design

Date: 2026-06-07
Status: approved — ready for implementation plan
Roadmap item: #1 in `2026-06-07-extensibility-roadmap.md`

## Problem

The only extension point around content writes is `EventSink`, which fires
*after commit*, is fire-and-forget, and cannot alter or reject the request.
A developer who wants per-content-type request-path logic — derive a field,
validate across fields, reject a payload, observe the saved record and fail
the response — must today fork the generic handler in
`crates/http/src/routes/content.rs`.

This design adds a `WriteHook` trait with two callbacks around the
create/update write path, wired in `crates/bin/src/main.rs` exactly like the
existing `EventSink` seam.

## Decisions (from brainstorming)

1. **before_write sees the raw JSON body** (before `body_to_binds`). It
   mutates a plain `serde_json::Map`; the framework then validates the
   returned body against the schema. Most flexible; injected values are
   re-validated.
2. **after_write runs after commit, observe-only.** It receives the final
   saved record as JSON and cannot roll back. Returning `Err` surfaces as a
   5xx (the write is already durable). Rollback-needing logic belongs in
   `before_write`. This keeps the hook decoupled from sqlx.
3. **Keep both WriteHook and EventSink, distinct roles.** `EventSink` stays
   fire-and-forget fan-out (webhooks, cache bust). `WriteHook.after_write` is
   for logic that needs the full saved record and may fail the response.
4. **Global trait, branch on context.** One `WriteHook` impl is wired in
   `main.rs`. The context carries `content_type`; the author dispatches per
   type in their own code (e.g. `if ctx.content_type == "article"`). Mirrors
   `EventSink`; no registry machinery.

## Chosen approach

Single `WriteHook` trait, two default-implemented methods, default `NoopHook`.
One `Arc<dyn WriteHook>` field on `AppState`. (Alternatives considered: two
separate traits — doubles plumbing; a hook `Vec` with framework-folded
composition — premature, needs ordering/short-circuit rules. Both deferred.)

## Trait and types

Added to `crates/http/src/state.rs`, beside `EventSink`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOp { Create, Update }

pub struct WriteContext<'a> {
    pub content_type: &'a str,
    pub operation: WriteOp,
    pub principal: &'a Principal,
}

#[async_trait]
pub trait WriteHook: Send + Sync + 'static {
    /// Runs after JSON parse, before schema validation (body_to_binds).
    /// May add/remove/rewrite fields, or return Err to reject the request.
    /// The returned body is validated against the schema by the framework.
    async fn before_write(
        &self,
        ctx: &WriteContext<'_>,
        body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        Ok(body)
    }

    /// Runs after commit. Receives the final saved record (post row_to_json,
    /// before populate / media-embed). Err surfaces as a 5xx; the write is
    /// already durable. For fire-and-forget fan-out use EventSink instead.
    async fn after_write(
        &self,
        ctx: &WriteContext<'_>,
        record: &Value,
    ) -> Result<(), Error> {
        Ok(())
    }
}

pub struct NoopHook;

#[async_trait]
impl WriteHook for NoopHook {}
```

`AppState` gains `pub hooks: Arc<dyn WriteHook>`.

## Call sites — `crates/http/src/routes/content.rs`

### create

- After `ensure` (authz) and the `ct` fetch, build
  `WriteContext { content_type: &ct.name, operation: WriteOp::Create, principal: &principal }`.
- `let body = state.hooks.before_write(&ctx, body).await.map_err(ApiError)?;`
  runs **before** `body_to_binds`, so the returned body is schema-validated.
- The rest of the write (verify targets, insert tx, write_links/media_links,
  commit) is unchanged. `row_to_json(&ct, &row)?` already produces the saved
  record into `body`.
- After `tx.commit()`, **before** `EventSink::emit`:
  `state.hooks.after_write(&ctx, &body).await.map_err(ApiError)?;`
- Then `events.emit(EntryCreated { .. })` and return as today.

### update

Same shape with `WriteOp::Update`:

- `before_write` immediately after the `ct` fetch, wrapping `body` before
  `body_to_binds`.
- The full-replace null-fill loop, verify, update tx, write_links, commit are
  unchanged.
- Compute the saved record once: `let record = row_to_json(&ct, &row)?;`
  pass `&record` to `after_write` after commit, then return `Json(record)`.

### Ordering rules (both operations)

- `before_write` runs **after** authz (`ensure`) — hooks only see authorized
  requests — and **before** `body_to_binds` — its output is schema-validated.
- `after_write` runs **after commit** and **before** `EventSink::emit` — the
  hook is the ordered, failable post-commit path; the event is fire-and-forget
  last.
- `after_write` runs **before** populate / media-embed enrichment, so it gets
  the plain persisted record, not the response-shaped expansion.

### Out of scope this iteration

`delete_one` gets no hooks. The `publish_entry` / `unpublish_entry` handlers
also do not fire hooks: they carry no request body (nothing for `before_write`
to transform or validate) and already emit `EntryUpdated` via `EventSink` for
observers. No per-type registry, no hook `Vec`/chain, no transaction access in
`after_write`. All deferred; recorded in the roadmap.

## Wiring and exports

- `crates/bin/src/main.rs`: add `hooks: Arc::new(NoopHook)` to the `AppState`
  literal (beside `events: Arc::new(NoopSink)`).
- `crates/http/src/lib.rs`: export `WriteHook`, `WriteContext`, `WriteOp`,
  `NoopHook` so the bin and downstream crates can implement and wire a hook.

## Error mapping

Hooks return `rustapi_core::Error`; the author selects the variant, mapped to
HTTP by the existing `ApiError` in `crates/http/src/error.rs`. No new error
variants.

- `before_write` reject → `Error::Validation(..)` → 422, or `Error::Forbidden`
  → 403. Author's choice. Runs before the tx, so nothing is written.
- `after_write` fail → typically `Error::Internal(..)` → 500. The write is
  already committed; the 500 signals a post-save step failed.

## Testing

Test-side `WriteHook` impls (counter / recorder / rejector) live in the http
crate tests, in the style of the existing `EventSink` / `Authz` test doubles
in `state.rs`.

1. Default `NoopHook` — create and update behavior unchanged (existing tests
   continue to pass).
2. `before_write` transforms — hook injects a field (e.g. `slug`); assert the
   persisted record contains it.
3. `before_write` rejects — hook returns `Error::Validation`; assert 422 and
   that **nothing was written** (hook runs before the tx).
4. `before_write` output is still schema-validated — hook injects an
   unknown/invalid field; `body_to_binds` rejects it (proves decision #1).
5. `after_write` observes — hook sees the saved record including server-set
   `id` and timestamps.
6. `after_write` fails — hook returns `Error::Internal`; assert 500 and that
   the **write was committed** (durable despite the hook error).
7. `WriteContext` correctness — `operation` is `Create` on POST and `Update`
   on PUT; `content_type` and `principal` are correct.

## Touch points summary

- `crates/http/src/state.rs` — `WriteOp`, `WriteContext`, `WriteHook`,
  `NoopHook`; `AppState.hooks` field.
- `crates/http/src/routes/content.rs` — `before_write` / `after_write` calls
  in `create` and `update`.
- `crates/http/src/lib.rs` — exports.
- `crates/bin/src/main.rs` — wire `NoopHook`.
