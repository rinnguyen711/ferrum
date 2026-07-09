# Docs Site (mdBook skeleton) — Design

## Goal

A narrative documentation website for Ferrum developers — guides, concepts, and
tutorials in the style of the Django / Strapi docs. This is distinct from the
auto-generated OpenAPI/Swagger UI served at `/docs`, which documents only the
dynamic `/api/{type}` endpoints. The docs site explains the framework: how to
install it, the mental model, and how to accomplish tasks.

v1 ships a **skeleton**: full navigation structure and a one-paragraph stub per
page, ready for prose to be filled in incrementally. No invented behavior — stubs
describe a page's scope, not specifics.

## Tooling

- **mdBook** — Rust-native static site generator (the Rust Book uses it). Single
  binary, markdown only, fast. Fits a Rust project; devs already have cargo.
- **Static output**, hosting decided later. No coupling to the axum binary in v1.

## Location

New top-level `book/` directory, sibling to `crates/`, `ui/`, `docs/`.

Rationale: `docs/` is owned by superpowers specs/plans. The published site stays
separate to avoid mixing internal planning docs with the public site.

## Structure

```
book/
  book.toml          # title "Ferrum", git repo link, default theme
  src/
    SUMMARY.md       # nav tree (mdBook table of contents)
    introduction.md
    getting-started/
      installation.md       # Docker + cargo
      first-run.md          # admin setup, login, JWT
      first-content-type.md # create first content type + entry
    concepts/
      content-types.md
      fields.md
      components.md
      relations.md
      draft-publish.md
      single-types.md
    guides/
      schema-as-code.md
      media-storage.md
      api-tokens.md
      roles.md
      webhooks.md
      write-hooks.md
      import-export.md
      graphql.md
    reference/
      rest-api.md
      graphql.md
      env-vars.md
      openapi.md            # links to live /docs Swagger UI
```

## Page contents (v1)

Each page is an H1 matching its nav title plus a single paragraph describing what
the page will cover. Each stub paragraph ends with a `<!-- TODO: fill -->` marker
so unfilled pages are greppable.

Stubs describe scope only. They do NOT assert specific behavior, flags, or
endpoints — that prose comes later, verified against code.

## Build

```sh
cd book
mdbook build      # → book/book/ static output
mdbook serve      # local preview at http://localhost:3000
```

`book/book/` (build output) is gitignored. Source under `book/src/` is committed.

## Out of scope (v1)

- Prose content (skeleton stubs only)
- Custom theme / CSS
- Search configuration beyond mdBook default
- Deploy / CI pipeline
- Serving the site from the ferrum binary

## Testing / verification

- `mdbook build` succeeds with no broken-link warnings (all `SUMMARY.md` entries
  resolve to existing files).
- Every `src/**/*.md` page appears in `SUMMARY.md` and vice versa.
