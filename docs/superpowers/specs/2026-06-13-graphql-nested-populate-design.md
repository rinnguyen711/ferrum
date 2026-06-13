# GraphQL Nested Relation/Media Population (selection-set driven)

Date: 2026-06-13
Status: design — approved, pending spec review

## Goal

Make GraphQL relation and media fields resolve to **nested objects** when a
client selects sub-fields (`author { name }`), instead of the v1 scalar-UUID
representation. Population is driven by the GraphQL selection set and reuses the
existing batched REST populate machinery — no N+1, no new query path.

## Context

The merged GraphQL surface (spec `2026-06-12-graphql-surface-design.md`) types
relation/media fields as scalar `UUID` ids, with nested population explicitly
deferred. That deferral was forced by a correctness bug: object-ref typing
produced dangling type references when a relation targeted a Single type
(Singles weren't surfaced as GraphQL objects), crashing `Schema::finish()`.

REST already does nested population well: `crates/http/src/populate.rs` parses a
`?populate=author,tags` param into `Vec<PopulateField>` and applies each via
**batched** queries (`apply_forward` / `apply_inverse` / `apply_many` — one
`WHERE id IN (...)` per relation across the whole page), mutating the row JSON
in place. The shared `content::list_entries` / `get_entry` already accept a
`populate: Option<&str>` and run this pipeline. The GraphQL resolvers currently
pass `populate: None`.

This work changes relation/media GraphQL types to objects, registers objects
for **all** content types (fixing the dangling-ref bug properly), and has the
list/get resolvers derive the populate set from the selection set so the
existing batched pipeline embeds nested objects before child resolvers run.

## Decisions (settled in brainstorming)

- **Relation/media fields are typed as objects, with `id` always present.**
  `author: Author`, `tags: [Tag!]`, `cover: Media`. A client wanting just the
  uuid writes `author { id }`. No separate `authorId` scalar field (redundant
  with the object's `id`).
- **One-level populate depth for v1.** Relations selected at the first level
  (directly on the entry) are populated; their own sub-relations are not.
- **Deeper sub-relations resolve to null, not error.** This is already how
  `json_field_resolver` behaves for absent keys — an unpopulated sub-relation
  is naturally null. No depth-walking/rejection logic. Forward-compatible:
  when deeper populate ships later, those nulls become data with no breaking
  change.
- **No N+1.** Population is eager and batched, driven by the selection set at
  the list/get resolver — never per-row in a child resolver.

## Architecture

### 1. Register an object per content type (all kinds) — `build.rs`

Today `build_schema` registers objects only for `Collection` types and skips
`Single`. Change: register a `build_output_object(ct)` for **every** content
type regardless of kind. Single types still get NO root Query/Mutation fields
(unchanged — they're not queryable as collections), but their object type is
registered so relations can reference it.

Relation/media field typing in `scalars.rs::base_type_name` reverts from the
v1 scalar `UUID` back to object refs:
- Relation → `pascal(target)` (the target type's object); list if m2m.
- Media → `Media` (re-register the `Media` object: `id: UUID!`, `url: String`,
  plus any media metadata already in the embedded media JSON — match what
  `media_embed` produces).

Because every content type now has a registered object, a relation to a Single
target resolves to a registered type — `Schema::finish()` no longer fails. This
is the proper fix for the deferred dangling-ref bug.

Input objects are unaffected: relation/media **inputs** stay scalar `UUID`
id(s) via `input_type_ref` (writes take ids, not nested objects) — unchanged.

### 2. Derive populate from the selection set — `resolve.rs`

`ResolverContext` derefs to `&Context`, which exposes `ctx.look_ahead() ->
Lookahead`. `Lookahead::field(name)` descends; `.selection_fields()` lists
selected fields; `.exists()` tests presence.

Add a helper that, given the `ResolverContext` and the `ContentType`, returns
the populate string for the directly-selected relation/media fields:

```rust
/// Walk the selection set one level into the entry object and collect the
/// names of relation/media fields the client selected. Returns a comma-joined
/// populate string (REST `?populate=` syntax) for content::list_entries.
fn populate_from_selection(ctx: &ResolverContext, ct: &ContentType) -> Option<String>;
```

- **List resolver** (`list_field`): the entry fields sit under
  `articles → data → <fields>`. So walk `ctx.look_ahead().field("data")`, take
  its `selection_fields()`, keep those whose name matches a relation/media
  field on `ct`, join with `,`. Pass as `populate` to `list_entries` (instead
  of `None`).
- **Get resolver** (`get_field`): entry fields sit directly under the field
  (`article → <fields>`). Walk `ctx.look_ahead()` selection fields directly.
  Pass to `get_entry`.

Only **relation and media** field names are eligible (look them up on `ct` by
kind). Scalar fields in the selection are ignored. Unknown/aliased fields are
ignored (`look_ahead` ignores aliases per its docs — acceptable; a v1 note).

The existing batched `apply_populate` (forward/inverse/many) then embeds each
selected relation as a nested object (or array) into the row JSON. Media embed
already runs unconditionally and produces the media object JSON.

### 3. Child resolvers unchanged — `json_field_resolver`

No change. Once the row JSON carries `author` as an embedded object (from
populate) instead of a uuid string, `json_field_resolver("author")` returns
that object as the child `parent_value`, and the `Author` object's field
resolvers read from it. A relation that was NOT selected for population is
absent from the JSON → `json_field_resolver` returns null → GraphQL null. This
is exactly the one-level-then-null behavior, with zero new code in the child
path.

### Data flow (list of articles selecting author)

```
articles(... ) resolver
  look_ahead: data { id title author { id name } }
  → relation fields selected: ["author"]
  → list_entries(.., populate = Some("author"))
      → batched apply_forward("author") embeds {author: {id, name, ...}} per row
  → envelope JSON has data[i].author as an object
Article.author resolver (json_field_resolver "author")
  → returns the embedded object → Author.name resolver reads it
Article.author.posts (if selected) → not populated → absent → null
```

## Error handling

- Populate parse errors (e.g. a relation field that can't be populated) surface
  through the existing `content::list_entries` error path → `gql_err` →
  `extensions.code`. Unchanged.
- Deeper-than-one-level selections never error — they resolve to null.
- The dangling-type class of failure is eliminated (all targets registered).

## Crate boundaries

All changes stay in `crates/http` (graphql + reuse of populate.rs). No changes
to core/sql/schema. `populate.rs` is reused as-is; no new public API there.

## Testing

Extend `crates/bin/tests/graphql.rs` (testcontainers):

- **forward relation populated when selected:** create `author` + `article`
  with a m2o relation; query `articles { data { title author { id name } } }`;
  assert `author` is an object with the right `name`, not a string.
- **relation NOT selected → field is just id-able:** query `author { id }`;
  assert the uuid comes back.
- **m2m relation populated as a list:** `tags { id name }` returns an array of
  tag objects.
- **media field populated:** a media field selected as `cover { id url }`
  returns the media object.
- **one-level cap:** `author { posts { title } }` — `author.posts` is null (not
  populated, no error); `author`'s own scalar fields resolve.
- **relation to a Single type does not break schema** (regression, generalizes
  the existing `relation_to_single_type_does_not_break_schema`): now `banner`'s
  `page` (relation to Single `homepage`) is selectable as an object
  (`page { id }`) and the schema builds.

Also add the v1-deferral coverage that the previous spec listed and we skipped:
enum field, JSON scalar field, DateTime/UUID scalar round-trip, and per-variant
error codes (CONFLICT, UNAUTHORIZED, BAD_USER_INPUT) — small, closes the test
gap noted in the final review.

Unit tests in `build.rs`/`scalars.rs`: relation/media base type is now the
object name (`Author`/`Media`), not `UUID`; Single types register an object but
no root field (assert SDL has `type Homepage` but not `homepages(`).

## Explicit non-goals (v1)

- **Multi-level nested populate** (author { posts { ... } }) — deferred; deeper
  levels are null. A later version walks the full selection tree with a depth
  cap.
- **Per-relation arguments** (filtering/paginating a nested relation list) — out
  of scope.
- **DataLoader** — not needed; the eager batched populate already avoids N+1 at
  one level.

## Touch points summary

- `crates/http/src/graphql/scalars.rs` — relation/media `base_type_name` back to
  object refs; update unit tests.
- `crates/http/src/graphql/build.rs` — register an object for every content type
  (incl. Single); re-register the `Media` object; Single types still get no root
  fields; update SDL tests.
- `crates/http/src/graphql/resolve.rs` — `populate_from_selection` helper;
  list_field/get_field pass derived populate instead of `None`.
- `crates/bin/tests/graphql.rs` — nested-populate tests + closed coverage gaps.
