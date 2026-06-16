# Writing Rustapi docs

Rules for adding to and updating the mdBook in `book/`. Read this before
touching any page under `src/`. Goal: every page stays consistent, accurate,
and runnable as the project evolves.

## Build & preview

```sh
cd book
mdbook serve --open   # live-reload at http://localhost:3000
mdbook build          # static output in book/book/ (gitignored)
```

Run `mdbook build` before committing doc changes. It fails on broken internal
links and missing `SUMMARY.md` entries — treat a failed build as a blocker.

## Structure

The book has four top-level sections. Put new pages in the right one:

| Section | Holds | Don't put here |
|---|---|---|
| Getting Started | Linear onboarding, run-once setup, first success | Deep concept theory, exhaustive options |
| Core Concepts | What a thing *is* and how it behaves | Step-by-step task recipes |
| Guides | Task-oriented "how do I X" recipes | Concept explanation, full API dumps |
| Reference | Exhaustive, lookup-oriented facts (endpoints, env vars) | Tutorials, opinions |

- Every page must be listed in `src/SUMMARY.md`. A file not in `SUMMARY.md` is
  invisible to readers — either link it or delete it.
- One page = one topic. If a page grows two distinct topics, split it.
- New section requires sign-off — don't add top-level `# Heading` groups to
  `SUMMARY.md` casually.

## Voice & style

- **Second person, task-oriented.** "You create a content type by…" not
  "A content type is created by…". The Reference section may be drier/neutral.
- **Imperative for steps.** "Run the migration." "Set `DATABASE_URL`."
- Short sentences. One idea per sentence. Cut filler (just/simply/basically).
- Define a term on first use, then link to its Core Concepts page after that —
  don't re-explain.
- US spelling. Sentence case for headings ("Schema as code", not "Schema As
  Code").

## Code examples — must be real and runnable

This is the strict rule. Docs lose trust the moment a snippet doesn't work.

- Every `curl`, JSON body, CLI command, and config snippet must match the
  **actual running API / binary**. Use real endpoint paths, real field names,
  real env-var names.
- Verify before you commit: run the snippet against a local server (`docker
  compose up`, then the request) or the actual CLI. If you can't run it, don't
  publish it.
- Show the command **and** a representative response when the response matters.
- Use realistic example data, not `foo`/`bar`. A blog `Article` with `title`
  and `body` beats `thing` with `field1`.
- Tag fenced blocks with a language (` ```sh `, ` ```json `, ` ```rust `,
  ` ```graphql `) so highlighting and theming work.
- Prefer relative paths/ports that match the README quickstart (`:8080` for the
  API, `:5173` for the UI dev server) so copy-paste works against defaults.

## Linking & reuse

- Link concepts and guides to each other instead of repeating content. Each
  fact has one home page; everywhere else links to it.
- **Don't duplicate the README.** Setup, Docker quickstart, env vars, and API
  examples live in `README.md`. Link to it; don't copy it.
- The REST surface is auto-generated — point readers to the live Swagger UI at
  `/docs` on a running server rather than hand-maintaining endpoint tables that
  will drift.
- Use relative links between pages (`[fields](concepts/fields.md)`), never
  absolute URLs to the published site.

## Theming

The book is themed with the rustapi design tokens via `theme/rustapi.css`.
Don't hard-code colors or inline styles in Markdown. If a page needs a new
visual treatment, change the token/CSS, not the page. See `DESIGN.md` at the
repo root for the token source of truth.

## Keeping docs current

- A code change that alters behavior, an endpoint, a field kind, or an env var
  **must update the affected page in the same PR.** Out-of-date docs are worse
  than missing docs.
- Naming note: the `rustapi` name is changing (rename pending). When the new
  name lands, do a deliberate sweep — title, intro, examples — don't trickle it.
- When you fill a stub, delete its `<!-- TODO: fill -->` marker. Grep for
  remaining TODOs before claiming the docs are done:

  ```sh
  grep -rn "TODO" book/src
  ```

## Page checklist

Before committing a page, confirm:

- [ ] Listed in `src/SUMMARY.md`
- [ ] In the correct section (concept vs guide vs reference)
- [ ] Second person, imperative steps, sentence-case headings
- [ ] Every snippet run and verified against a real server/CLI
- [ ] Fenced blocks have language tags
- [ ] Links instead of duplicated content (esp. README)
- [ ] No `<!-- TODO: fill -->` left behind
- [ ] `mdbook build` passes clean
