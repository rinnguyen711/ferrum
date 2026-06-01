# UI Design-Match + Default Types & Seed — Design

Date: 2026-06-02
Status: Approved (pending spec review)

## Goal

Bring the admin UI up to the visual + functional level of the reference design in
`design/` (React reference impl + screenshots), leave clear placeholders for
features not yet backed by the API, and make a fresh install ship with the three
default collection types (Article, Author, Category) plus seed data so the
"docker compose up" demo shows a populated CMS.

## Scope

Three workstreams:

1. **Backend bootstrap/seed** — on startup, if the DB has no content types, create
   Article / Author / Category and insert sample rows (with real relations).
2. **UI design-match** — upgrade existing screens (Dashboard, Content Manager list,
   Content-Type Builder, Settings shell, secondary panels) to the design's layout
   and add the missing screens (Media Library) as placeholders.
3. **Placeholders** — render not-yet-implemented surfaces as visible, clearly-marked
   "preview / coming soon" UI rather than hiding them.

Out of scope: real media upload, real token CRUD, real user/role system, webhooks,
i18n, single-type editing, component types. These appear as placeholders only.

## Decisions (from brainstorming)

- **Seeding:** backend bootstrap on startup (gated behind empty-DB check + a
  `RUSTAPI_SEED` flag, default on for the demo). Real, persistent, works for raw API.
- **Relations:** wire real relations — seed authors + categories, capture their UUIDs,
  seed articles pointing at them.
- **Identity:** generic "Admin" identity in shell (not the fictional "Mara Velez").
- **Placeholders (all visible):** Media Library, Single types & Components,
  Settings sub-pages, Dashboard stat extras.

## Architecture

### 1. Backend seed

New module `crates/bin/src/seed.rs`, called from `main.rs` after
`registry.reload_from_db` + `SchemaService` construction, before the router is built.

```
pub async fn seed_if_empty(pool: &PgPool, schemas: &SchemaService, cfg: &Config) -> Result<()>
```

Flow:
1. If `cfg.seed` is false → return.
2. If `schemas.registry().all().await` is non-empty → return (idempotent; never
   touches an existing dataset).
3. Build three `NewContentType` values matching `design/rustapi/data.js` schemas,
   mapped to real `FieldKind`s:
   - Article: title (String, req), slug (Slug, req), status (Enum draft/review/published, req),
     excerpt (Text), body (Text, req), author (Relation→author, req),
     categories (Relation→category), featured (Boolean), read_time (Integer),
     published_at (Datetime). (Media/rich-text not modeled by the API → use Text/String;
     `cover` omitted — no media kind.)
   - Author: name (String, req), role (String), bio (Text),
     articles (Relation→article, inverse of Article.author).
   - Category: name (String, req), slug (Slug, req), color (String), description (Text).
   Create order matters for relation cross-ref validation: **author, category, article**
   (article references both). Author.articles inverse is declared on the author type;
   verify against `validate_relation_cross_refs` ordering — if inverse-before-target
   fails, declare the inverse relations via a follow-up `patch` after both types exist,
   or omit the inverse `articles` field (it is populate-only convenience, not required).
4. Insert rows by reusing the existing write path helpers:
   - `rustapi_http::entry::body_to_binds(&ct, body_map, true)` → binds + relation checks
   - `rustapi_sql::insert(&ct, &binds)` → SQL + binds
   - `bind_all(sqlx::query(&sql), &binds).fetch_one(pool)` → returns the row; capture `id`.
   Seed authors first (capture id by name), categories next (capture id by name),
   then articles referencing those captured UUIDs for `author` and `categories`.
   Data drawn from `design/rustapi/data.js` (4 authors, 5 categories, 10 articles).
5. Log a one-line summary (`tracing::info!`) of what was created.

Config: add `seed: bool` to `crates/bin/src/config.rs`, env `RUSTAPI_SEED`
(default `true`). Document in README + docker-compose comment.

Errors during seed are logged but **non-fatal** unless a hard DB error — the server
should still boot. (A failed type-create that's a `Conflict` means data already
exists → treat as already-seeded, continue.)

### 2. UI design-match

The shell (`rs-app` rail + secondary panel + topbar) already matches the design.
CSS classes for the richer screens largely exist in `ui/src/styles.css`; the gap
list (`rs-builder-empty`, `rs-radio-cards`, `rs-rel*`, `rs-setting-row`, modal
classes, etc.) will be ported from `design/rustapi/styles.css` where missing.

Per-screen changes:

- **Dashboard** (`screens/Dashboard.tsx`): replace the bare card grid with the
  design's layout — hero greeting ("Workspace" eyebrow + generic greeting), stat
  grid (Published / In review / Drafts from real counts across types where it makes
  sense, p99 latency static-mock), "Recently edited" list (real recent entries if a
  default type exists; else the content-type cards), and a "System" panel
  (API health from `/healthz` = real; DB/build/webhooks/sparkline = static mock,
  visibly cosmetic). Hero "New article" button links to `/content/article/new` when
  that type exists, else hidden.

- **Content Manager list** (`screens/ContentList.tsx`): add the status tab bar
  (All / Published / In review / Draft) — wired only when the type has a `status`
  enum field, else hidden; toolbar (search input, Filters/Sort/Fields buttons —
  search is real client-side filter; Filters/Fields are placeholder ghost buttons);
  richer table cells (title emphasis, status badge, relation/author avatar where a
  relation resolves, category chips); bulk-select bar (checkboxes real selection,
  bulk actions are placeholder); pager (real "showing N of total", page controls
  placeholder). Generic across types — no per-type hardcoding like the design's
  ArticleList/AuthorList; drive everything off the schema + entries already loaded.

- **Content-Type Builder** (`builder/SchemaEditor.tsx`): align header to design
  (`api::name.name · N fields · collection type`), empty-state ("Add your first
  field"), schema rows with drag handle (visual only), field-type pill + meta,
  edit/delete actions. Keep the existing real create/patch/draft machinery.
  "Preview API" button = placeholder.

- **Media Library** (`screens/MediaLibrary.tsx`, new): static grid from the
  design's mock assets, marked as a preview; rail icon re-enabled, route
  `/media` renders it instead of redirecting home. "Upload" button = placeholder.

- **Secondary panel** (`components/shell.tsx`): add Single types (Homepage, Global)
  and Components (SEO, Call to action) groups as disabled "coming soon" items in
  both Content + Builder panels; add the panel search input (placeholder/visual).
  Settings panel keeps its groups; non-API-tokens items disabled.

- **Settings** (`screens/Settings.tsx`): render the API tokens table as a static
  placeholder (design's mock tokens), clearly non-functional ("Create new token" =
  placeholder). Other settings sub-pages = disabled panel items.

- **Shell identity** (`components/shell.tsx`): replace "Mara Velez / Editor in chief"
  with generic "Admin / API key" (avatar initials "AD" or a generic glyph).

### 3. Placeholder convention

A small shared affordance: placeholder buttons get a `data-placeholder` /
`title="Coming soon"` and a muted style; non-functional sections show a subtle
"Preview" pill. Disabled panel items use `disabled` + muted styling. Goal: a viewer
can tell at a glance what's live vs. mocked. No fake success toasts.

## Data flow

- Seed runs once at boot → persisted rows in Postgres → served by the existing
  `/api/:type` + `/admin/content-types` endpoints → UI loads them through the
  current `endpoints.ts` functions. No new API surface.
- Dashboard "real" numbers come from `listContentTypes` + `listEntries` counts;
  health from `/healthz`. Everything else on the dashboard is labeled static mock.

## Error handling

- Seed: non-fatal, logged; `Conflict` = already seeded. Hard DB connection errors
  propagate (server already failed to connect earlier anyway).
- UI: existing per-screen loading/error/retry patterns are reused. Placeholder
  controls never call the API.

## Testing

- Backend: an integration test (testcontainers Postgres, same harness as existing
  `crates/bin/tests`) that runs `seed_if_empty` on a fresh DB and asserts: 3 content
  types exist, author/category/article row counts match, and a sample article's
  `author` populate resolves to a real author. A second call is a no-op (idempotent).
- UI: typecheck/build (`pnpm build`) clean; manual screenshot pass against the
  design screenshots for Dashboard, Content list, Builder, Media, Settings.
- `cargo build --workspace` + `cargo test --workspace` green.

## Open risk

Relation inverse ordering in seed (Author.articles ↔ Article.author). Mitigation in
the seed flow above (declare inverse after both types exist, or drop the inverse
convenience field). Resolve during implementation against
`validate_relation_cross_refs`.
