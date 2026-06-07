# Extensibility Roadmap

Date: 2026-06-07
Status: roadmap (item #1 scheduled for implementation next)

## Context

rustapi is a typed, runtime-schema headless CMS. It adapts well as a
drop-in CRUD API + admin for the common headless-CMS shape: runtime schema
CRUD, rich field kinds (relation, media, enum, json, email/url/slug),
Strapi-style filters + pagination + populate, draft/publish, JWT auth,
RBAC, OpenAPI generation, pluggable media storage.

Where it falls short today is **developer extension in the request path**.
Adding custom domain logic for a content type currently means forking the
generic handler in `crates/http/src/routes/content.rs`. The seams that do
exist (`EventSink`, `Authz`, `StorageProvider`) are wired in
`crates/bin/src/main.rs` and only cover side effects, coarse access, and
media — not request-path behavior.

This document records the prioritized gaps. Item #1 (write hooks) is the
highest-leverage fix and will be designed + implemented in a follow-up.

## Improvements (prioritized)

### 1. Write hooks — `before_write` / `after_write` (NEXT)

**Gap.** The only extension point around writes is `EventSink`, which fires
*after commit*, is fire-and-forget, and cannot alter or reject the request.
There is no hook to:

- compute / derive fields (e.g. slug from title, totals)
- run cross-field or async validation (uniqueness beyond a DB constraint,
  remote checks)
- mutate or reject the payload before it is persisted
- enforce ownership-on-write

**Direction.** A trait, wired in `main.rs` like `EventSink`, invoked by the
content handler inside the write path:

```
before_write(ctx) -> Result<Body, Error>   // may transform or reject
after_write(ctx)  -> Result<(), Error>      // runs in-transaction, can roll back
```

`ctx` carries content type, principal, operation (create/update), and the
parsed body. Default impl is a no-op so existing behavior is unchanged.

Touch points: `crates/http/src/state.rs` (trait + `AppState` field),
`crates/http/src/routes/content.rs` (`create` / `update` call sites),
`crates/bin/src/main.rs` (wire default no-op).

See follow-up design doc for the full contract.

### 2. Injectable custom routers

**Gap.** `build_router` in `crates/http/src/routes/mod.rs` merges a fixed
list of sub-routers. Custom endpoints (e.g. `POST /api/article/:id/feature`)
require editing the core crate.

**Direction.** Let the bin pass `extra_routers: Vec<Router<AppState>>` (or a
builder) merged into the protected router. Custom endpoints without a fork.

### 3. Row / field-level authz

**Gap.** `Authz::can(principal, action, content_type)` in
`crates/http/src/state.rs` is type-level only. No per-record rules
(`author == me`), no field-level visibility, no conditions.

**Direction.** Extend the authz contract to receive the record (or a
predicate to push into the query) so ownership and conditional access become
expressible. Needed for real multi-user / multi-tenant use.

### 4. Async event delivery (outbox + worker)

**Gap.** `EventSink::emit` is awaited inline in the request path and is
fire-and-forget — a slow sink slows the response, and there is no retry or
delivery guarantee.

**Direction.** Persist events to an outbox table inside the write
transaction; a background worker delivers with retry/backoff. Reliable
webhooks at scale.

### 5. Structured / component field types

**Gap.** The `Json` field kind is "arbitrary jsonb, no schema validation" —
an escape hatch with no guard rails. No nested / repeatable structured types
(Strapi components, Sanity blocks).

**Direction.** A validated structured field kind with a declared inner shape
(reusable component definitions, repeatable groups).

## Notes

- Items #1 and #2 together move the project from "use as-is" to "build on
  top" — after them, custom domain logic no longer means forking the handler.
- All seams stay compile-time / Rust-wired (no dynamic plugin loading). That
  is acceptable for a single team owning the binary; a marketplace / dynamic
  plugin story is explicitly out of scope here.
