# UI Consistency Pass — Design

**Date:** 2026-06-07
**Scope:** Admin UI (`ui/`). Cross-page visual consistency + DESIGN.md token compliance. No layout
redesign, no behavior change, no new features.

## Problem

The admin UI was built screen-by-screen. Shared patterns drifted: the same status pill is rendered
three ways, the editor back-bar is hand-rolled in three screens, error banners reuse a class named
`rs-login-error` everywhere with inline margin hacks, and several colors are hard-coded hexes that
DESIGN.md says must be tokens. This pass unifies those without changing what any screen *does*.

DESIGN.md is the source of truth and stays authoritative; this work brings the code into line with it
and adds the few tokens DESIGN.md references but the stylesheet never defined (notably Danger).

## Findings (audited 2026-06-07)

### Color / token violations
- `#DC2626` hard-coded 6× (`styles.css` lines ~190, 670, 671, 764, 765, `rs-input--err`). DESIGN.md §2
  lists Danger `#DC2626` but no `--danger` token exists.
- `rs-login-error` uses raw `#b91c1c` — a different red from every other danger surface.
- `rs-settings-ok` uses raw `#1a7f4b` instead of `--ok`.
- `rs-field-error` references `var(--danger, #c0392b)` — a *third* red, and the token it names doesn't
  exist.
- Login card hard-codes warm fallbacks `#faf8f5` / `#e7e2da` / `#fff` that predate the grey ramp.
- Avatar neutral fill `#52525B` inlined 6× across `shell.tsx`, `Users`, `Roles`, `RoleDetail`,
  `ContentList`.

### Component / markup inconsistencies
- **Status pill, 3 renderings:** `StatusBadge` (`shell.tsx`, dot + label) vs. raw `rs-status` spans in
  `ContentList.tsx:272-273` and `EntryEditor.tsx:146` (no dot, hand-built label).
- **Error/notice banner:** class `rs-login-error` used in 14 places as a generic banner; every
  non-login use carries an inline `style={{ margin… }}` because the class has no layout. `rs-settings-ok`
  is a parallel one-off for the success case.
- **Editor back-bar** hand-rolled in `UserEditor`, `RoleDetail`, `EntryEditor` (same
  `rs-editor-bar` + `rs-back` + title + actions). `MediaSettings` uses a different "Back to Media" ghost
  button in `rs-cm-head` instead.
- **Checkbox** triplicated: `media/Checkbox.tsx`, a local `Checkbox` in `ContentList.tsx:298`.
- **`initials()`** defined locally in `ContentList.tsx:312`; inlined as `.slice(0,2).toUpperCase()`
  elsewhere — two different rules.
- **Loading / empty copy** varies: "Loading…", "Failed to load users.", "Couldn't load type…", plus
  `MediaLibrary` has no loading state at all.

### Dead / duplicate CSS
- `rs-count-pill` defined twice (lines ~778 and ~790).
- `rs-radio-dot` defined twice (lines ~652 and ~746).
- `rs-danger` color rule appears 3×.

## Changes

### 1. Tokens (`styles.css`)
Add to `:root` and `[data-theme="dark"]`:
- `--danger`, `--danger-bg`, `--danger-text` (dark brightens text like `--accent-text` does).
- `--avatar-neutral` — replaces inlined `#52525B`.

Route every `#DC2626` / `#b91c1c` / `#c0392b` to `--danger`; `#1a7f4b` to `--ok`. Fix the Login card to
use `--bg` / `--surface` / `--border` (drop the warm fallbacks).

### 2. Rename `rs-login-error` → `rs-notice`
- `.rs-notice` (default = error tone) with `.rs-notice--ok` modifier; built-in block margin so the 14
  inline `style` hacks are deleted.
- Fold `rs-settings-ok` into `.rs-notice--ok`.
- Update all 14 usages. Field-level `rs-err-msg` / `rs-field-error` stay (distinct purpose) but use
  `--danger`.

### 3. Shared TSX primitives — new `ui/src/components/ui.tsx`
- `<Notice tone="error" | "ok">{children}</Notice>` — replaces all `rs-login-error` / `rs-settings-ok`
  sites.
- `<LoadingState />` and `<EmptyState>` — one "Loading…" string and one empty-state wrapper
  (`rs-empty`). Screens adopt these for their load/error/empty branches; copy unified to
  "Loading…" / "Couldn't load <thing>." / "<noun> not found."
- `<EditorBar back title status actions />` — the `rs-editor-bar` block. `EntryEditor`, `UserEditor`,
  `RoleDetail` adopt it. `MediaSettings` keeps its `rs-cm` page layout (it is not a bare editor) but its
  back button is normalized to the same affordance/label style.
- Promote one `Checkbox` (move to `ui.tsx` or keep `media/Checkbox.tsx` as the canonical and import it);
  delete the `ContentList` local copy.

### 4. Helpers (`util.ts`)
- `initials(name: string): string` — single rule; replaces `ContentList` local copy and the inlined
  `.slice(0,2)` calls in avatar usages.
- `AVATAR_NEUTRAL` constant bound to `var(--avatar-neutral)` (or pass the token via CSS) — replaces the
  6× `#52525B`.

### 5. Status pills
Route `ContentList` and `EntryEditor` published/draft pills through `StatusBadge`. **Decision: dot
everywhere** — every status pill in the app renders identically (leading dot + label). Remove the raw
`rs-status` spans.

### 6. Dead / duplicate CSS
Remove the duplicate `rs-count-pill` and `rs-radio-dot` blocks; consolidate `rs-danger` to one rule.

## Non-goals
- No layout, spacing, or density changes.
- No new screens, fields, or API wiring.
- No change to `MediaLibrary`'s grid/folder UX beyond adopting shared notice/loading primitives.
- DESIGN.md prose is not rewritten; if a new token is added it is reflected in the §2 table.

## Verification
- `cd ui && pnpm typecheck` — clean.
- `cd ui && pnpm build` — succeeds.
- Visual check via the playwright skill: walk each page (Dashboard, Content list + entry editor, Builder,
  Media library + settings, Users, Roles, Role detail, User editor, Login, Settings) in **light and
  dark**, confirming status pills, notices, back-bars, and avatars render consistently and no color
  regressed.
- Grep guard: no remaining `rs-login-error`, no remaining `#52525B` / `#b91c1c` / `#c0392b` / `#1a7f4b`
  in `ui/src`, no inline `style={{ margin…` on notice banners.
