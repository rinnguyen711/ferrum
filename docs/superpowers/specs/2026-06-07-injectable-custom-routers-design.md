# Injectable Custom Routers — Design

Date: 2026-06-07
Status: approved, ready for implementation
Roadmap item: #2 in `2026-06-07-extensibility-roadmap.md`

## Problem

`build_router` in `crates/http/src/routes/mod.rs` merges a fixed list of
sub-routers. Adding a custom endpoint (e.g. `POST /api/article/:id/feature`)
means editing the core `http` crate. There is no seam for the bin to mount its
own request-path endpoints.

## Goal

Let the bin pass custom routers into `build_router`, merged behind the same
auth layer as the built-in protected routes — custom domain endpoints without
forking the core crate.

## API change

`build_router` gains one parameter:

```rust
pub fn build_router(state: AppState, extra: Vec<Router<AppState>>) -> Router
```

Each router in `extra` merges into the **protected** router, *after* the
built-in sub-routers, before the `require_auth` `route_layer` is applied:

```rust
let mut protected = Router::new()
    .merge(schema::router())
    .merge(content::router())
    .merge(users::router())
    .merge(media::router())
    .merge(auth::protected_router());

for r in extra {
    protected = protected.merge(r);
}

let protected = protected.route_layer(axum::middleware::from_fn_with_state(
    state.clone(),
    require_auth,
));
```

Consequences of this placement:

- Extra routes sit behind `require_auth` — they inherit the same JWT gate as
  every other `/api` and `/admin` endpoint. A caller cannot accidentally mount
  an unauthenticated endpoint through this seam.
- Path collisions panic at startup. Axum's `.merge` panics on a duplicate
  exact path + method; there is no silent-override precedence. A custom route
  that clashes with a built-in fails loudly at boot, not at request time.
- Custom endpoints with distinct literal paths (`/api/article/:id/feature` vs
  the generic `/api/:type/:id`) do not collide — different first segment.

## Scope (YAGNI)

- **Protected-only.** No separate parameter for public (unauthenticated)
  custom routers. The stated gap is auth-gated custom endpoints; a public-extras
  seam can be added later if a real need appears.
- **Plain `Vec<Router<AppState>>`, not a builder struct.** Matches the spec
  wording, keeps the diff minimal, and fits the existing flat-function style of
  `build_router`. A builder would add surface area without a current consumer.

## Callers updated

- `crates/bin/src/main.rs` — `build_router(state, vec![])`.
- `crates/bin/tests/common/mod.rs` — `build_router(state, vec![])`.

Both pass an empty vec; behavior is unchanged for the shipped binary and the
existing test suite.

## Testing

New integration test (in the `http` crate's router tests, or `bin` tests
alongside `common`):

1. Build a router with one extra route, e.g. `GET /api/_probe` returning `200`
   with a body derived from `AppState` (proving the `State<AppState>` extractor
   resolves on an injected route).
2. Assert the probe is reachable **with** a valid token → `200`.
3. Assert the probe returns `401` **without** a token → confirms injected
   routes inherit the `require_auth` layer.

This covers both halves of the contract: the merge works, and extras are
auth-gated like built-ins.

## Out of scope

- Public/unauthenticated custom routers.
- A builder API or fluent configuration object.
- Dynamic / runtime router loading — all seams stay compile-time Rust-wired,
  consistent with the roadmap's stated constraint.
