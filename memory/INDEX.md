# Memory Index

Knowledge files for this project. Read the matching file when a topic comes up — not before.

| File | Topics / keywords |
|------|-------------------|
| [architecture.md](architecture.md) | system design, crates, services, data flow, axum router, sqlx, event sink, builder draft context, SaveBar, content list query flow, filters, RoleRegistry cache, RoleAuthz, custom roles, permissions |
| [decisions.md](decisions.md) | trade-offs, why X over Y, rationale, rejected approaches, server-side filtering, floating save bar, tab counts, no UI test infra, per-type permissions, role cache vs query, system roles locked, user field scope, GraphQL dynamic schema, GqlRegistry, relation scalar ids, async-graphql-axum pin, keyset vs offset pagination, cursor=first sentinel, sort_col id tiebreak, opaque cursor token, count opt-out, no offset cap, configurable pool |
| [gotchas.md](gotchas.md) | bugs, edge cases, surprises, footguns, PATCH vs PUT, test setup, dev creds, compose stack, loader vs popovers, design CSS porting, sqlx migrate rebuild, test flake, tests in bin/tests, AppState construction sites, GraphQL relation dangling type, gql 503, axum 0.7 vs 0.8, REST vs GraphQL side-effect blind spot, CSV formula injection, two settings sidebars, keyset text-cast 422, datetime seek bind OID, ordering seam tiebreak, non-scalar sort 500, whole-branch review catches cross-cutting bug, bench branch behind main, memory wiped by branch merge |
