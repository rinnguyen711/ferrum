# UI Consistency Pass Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the admin UI into cross-page consistency and DESIGN.md token compliance — one status-pill renderer, one notice banner, one editor back-bar, one checkbox, shared helpers, and real Danger tokens — with no layout or behavior change.

**Architecture:** Add the missing CSS tokens (`--danger*`, `--avatar-neutral`) and a renamed `.rs-notice` banner to `styles.css`. Add shared React primitives in a new `ui/src/components/ui.tsx` (`Notice`, `LoadingState`, `EmptyState`, `EditorBar`) and a canonical `Checkbox`, plus `initials()`/`AVATAR_NEUTRAL` in `util.ts`. Then migrate each screen to the shared pieces and delete the duplicates.

**Tech Stack:** React 18 + TypeScript, plain CSS (token-based design system), Vite. No component test harness — verification is `pnpm typecheck`, `pnpm build`, and grep guards, run from `ui/`.

**Conventions for every command below:** run from `/Users/rinnguyen/projects/rustapi/ui` unless stated. The repo `.gitignore` ignores `/docs`, so plan/spec commits already happened with `-f`; source commits under `ui/` are normal.

**No-regression guard used throughout:**
```bash
pnpm typecheck && pnpm build
```
Expected: typecheck prints nothing and exits 0; build ends with `✓ built in …` and writes `dist/`.

---

## Task 0: Branch + baseline

**Files:** none (already on branch `ui-consistency-pass`).

- [ ] **Step 1: Confirm branch and clean baseline**

Run (from repo root `/Users/rinnguyen/projects/rustapi`):
```bash
git branch --show-current && git status --short
```
Expected: `ui-consistency-pass`, and the only changes are ` M .gitignore` (pre-existing). If not on the branch, run `git checkout ui-consistency-pass`.

- [ ] **Step 2: Capture baseline build passes**

Run (from `ui/`):
```bash
pnpm typecheck && pnpm build
```
Expected: both succeed. This proves the starting point is green so later failures are attributable to this work.

---

## Task 1: CSS tokens — Danger + avatar neutral

**Files:**
- Modify: `ui/src/styles.css` (`:root` ~lines 21-32, `[data-theme="dark"]` ~lines 47-54)

- [ ] **Step 1: Add Danger + avatar tokens to `:root`**

In `:root`, immediately after the `--warn` / `--muted-bg` block (the line `--muted-bg: #EFEFF1;`), add:
```css
  --danger: #DC2626; --danger-bg: #FBEAEA; --danger-text: var(--danger);
  --avatar-neutral: #52525B;
```

- [ ] **Step 2: Add dark overrides**

In `[data-theme="dark"]`, after the `--muted-bg: #26262C;` line, add:
```css
  --danger: #F87171; --danger-bg: #2A1414; --danger-text: color-mix(in srgb, var(--danger) 88%, white);
  --avatar-neutral: #52525B;
```

- [ ] **Step 3: Verify build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass (CSS-only change; tokens not yet referenced).

- [ ] **Step 4: Commit**

```bash
git add src/styles.css
git commit -m "feat(ui): add --danger and --avatar-neutral design tokens"
```

---

## Task 2: CSS — route hard-coded reds/greens through tokens

**Files:**
- Modify: `ui/src/styles.css` (lines ~190, 486-488, 670-671, 764-765, 849-850, and `rs-input--err`)

- [ ] **Step 1: Replace `#DC2626` danger usages**

Replace each occurrence as follows (use Edit, one block at a time; the surrounding rule text is shown for uniqueness):

`rs-danger` hover (~line 190):
```css
.rs-danger:hover { color: var(--danger) !important; border-color: color-mix(in srgb, var(--danger) 40%, var(--border)) !important; }
```

`rs-input--err` (~line 670):
```css
.rs-input--err { border-color: var(--danger) !important; box-shadow: 0 0 0 3px color-mix(in srgb, var(--danger) 14%, transparent) !important; }
```

`rs-err-msg` (~line 671):
```css
.rs-err-msg { font-size: 12px; color: var(--danger); font-weight: 500; }
```

`rs-danger` base + hover (~lines 764-765):
```css
.rs-danger { color: var(--danger); }
.rs-danger:hover { color: var(--danger); }
```

`rs-link-btn.rs-danger:hover` (~line 375):
```css
.rs-link-btn.rs-danger { color: var(--text-muted); } .rs-link-btn.rs-danger:hover { color: var(--danger); }
```

- [ ] **Step 2: Fix `rs-field-error` phantom token + `rs-settings-ok` raw green**

`rs-field-error` (~line 849):
```css
.rs-field-error { display: block; margin-top: 4px; font-size: 12px; color: var(--danger); }
```

`rs-settings-ok` (~line 850) — will be superseded by `.rs-notice--ok` in Task 3, but normalize now so the file is token-clean if anything still references it:
```css
.rs-settings-ok { margin-top: 4px; padding: 8px 12px; border-radius: 8px; font-size: 13px; color: var(--ok); background: var(--ok-bg); }
```

- [ ] **Step 3: Verify no raw danger/ok hexes remain (except the `:root` token definitions)**

Run:
```bash
grep -nE '#b91c1c|#c0392b|#1a7f4b' src/styles.css
grep -nE '#DC2626' src/styles.css
```
Expected: first command prints **zero** matches. Second command prints **exactly one** line — the `--danger: #DC2626;` token definition in `:root` (the only allowed literal). If `#DC2626` appears anywhere else, a usage was missed.

- [ ] **Step 4: Verify build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass.

- [ ] **Step 5: Commit**

```bash
git add src/styles.css
git commit -m "refactor(ui): route danger/ok colors through tokens"
```

---

## Task 3: CSS — rename `rs-login-error` → `rs-notice`, fix Login card

**Files:**
- Modify: `ui/src/styles.css` (lines ~465-488)

- [ ] **Step 1: Replace the `.rs-login` block**

Replace the existing block (from `.rs-login {` through the closing brace of `.rs-login-error`) with:
```css
.rs-login {
  display: grid;
  place-items: center;
  min-height: 100vh;
  background: var(--bg);
}
.rs-login-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: 320px;
  padding: 28px;
  border: 1px solid var(--border);
  border-radius: var(--r-lg);
  background: var(--surface);
}
.rs-login-card h1 {
  margin: 0;
  font-size: 20px;
}

/* notice banner — error (default) + ok tones */
.rs-notice {
  margin: 0 0 12px;
  padding: 9px 13px;
  border-radius: var(--r-md);
  font-size: 13px;
  font-weight: 500;
  color: var(--danger-text);
  background: var(--danger-bg);
  border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
}
.rs-notice--ok {
  color: var(--ok);
  background: var(--ok-bg);
  border-color: color-mix(in srgb, var(--ok) 30%, transparent);
}
```

Note: `rs-notice` now owns its bottom margin, so the inline `style={{ margin… }}` hacks get deleted in Tasks 5-7. Login used `rs-login-error` with no margin and was the last child — the new bottom margin is harmless there (gap layout). The old hard-coded `border-radius: 12px` becomes `--r-lg`.

- [ ] **Step 2: Verify build (class not yet used → expect unused-CSS only, still passes)**

Run: `pnpm typecheck && pnpm build`
Expected: both pass. `.rs-login-error` no longer exists in CSS but is still referenced in TSX — that's fine for CSS (unknown class = no style), it's fixed in Tasks 5-7. Build does not fail on missing CSS classes.

- [ ] **Step 3: Commit**

```bash
git add src/styles.css
git commit -m "refactor(ui): rename rs-login-error to rs-notice, tokenize login card"
```

---

## Task 4: Shared helpers in `util.ts`

**Files:**
- Modify: `ui/src/util.ts`

- [ ] **Step 1: Read current util to match style**

Run: `sed -n '1,40p' src/util.ts` (just to see exports/format; do not edit yet).

- [ ] **Step 2: Append helpers**

Add to the end of `ui/src/util.ts`:
```ts
/** Neutral avatar fill, bound to the --avatar-neutral design token. */
export const AVATAR_NEUTRAL = "var(--avatar-neutral)";

/** Derive up to two uppercase initials from a name or email. */
export function initials(s: string): string {
  const base = s.includes("@") ? s.split("@")[0] : s;
  const parts = base.split(/[\s._-]+/).filter(Boolean);
  const letters = parts.length >= 2
    ? parts[0][0] + parts[1][0]
    : base.slice(0, 2);
  return letters.toUpperCase() || "?";
}
```

Note: existing avatar call sites use `email.slice(0,2).toUpperCase()`. `initials("name@company.com")` → `"NA"` (same first two letters of local part), so the visual result is unchanged for emails while names like "Jane Doe" now yield "JD".

- [ ] **Step 3: Verify build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass (new exports, unused so far).

- [ ] **Step 4: Commit**

```bash
git add src/util.ts
git commit -m "feat(ui): add initials() and AVATAR_NEUTRAL helpers"
```

---

## Task 5: Shared primitives `components/ui.tsx` + canonical Checkbox

**Files:**
- Create: `ui/src/components/ui.tsx`
- Modify: `ui/src/screens/media/Checkbox.tsx` (re-export from canonical to avoid breaking media imports)

- [ ] **Step 1: Create `ui/src/components/ui.tsx`**

```tsx
import type { ReactNode } from "react";
import { Icons } from "./icons";

/** Inline error/ok banner. Owns its own bottom margin. */
export function Notice({
  tone = "error",
  children,
}: {
  tone?: "error" | "ok";
  children: ReactNode;
}) {
  return <div className={"rs-notice" + (tone === "ok" ? " rs-notice--ok" : "")}>{children}</div>;
}

/** Centered loading placeholder, consistent copy. */
export function LoadingState({ label = "Loading…" }: { label?: string }) {
  return <div className="rs-empty">{label}</div>;
}

/** Centered empty / error placeholder with optional action. */
export function EmptyState({ children }: { children: ReactNode }) {
  return <div className="rs-empty">{children}</div>;
}

/** Canonical checkbox (button + check glyph). */
export function Checkbox({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      className={"rs-check" + (checked ? " is-on" : "")}
      onClick={onChange}
    >
      {checked && <Icons.check size={13} />}
    </button>
  );
}

/** Editor top bar: back button + title (+ optional status) + actions. */
export function EditorBar({
  onBack,
  title,
  status,
  actions,
}: {
  onBack: () => void;
  title: ReactNode;
  status?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="rs-editor-bar">
      <button className="rs-back" onClick={onBack} aria-label="Back">
        <Icons.arrowLeft size={18} />
      </button>
      <div className="rs-editor-titlewrap">
        <h1>{title}</h1>
        {status}
      </div>
      {actions && <div className="rs-editor-actions">{actions}</div>}
    </div>
  );
}
```

- [ ] **Step 2: Point `media/Checkbox.tsx` at the canonical one**

Replace the entire contents of `ui/src/screens/media/Checkbox.tsx` with:
```tsx
export { Checkbox } from "../../components/ui";
```
This keeps every `import { Checkbox } from "./media/Checkbox"` / `"./Checkbox"` working while there is one implementation.

- [ ] **Step 3: Verify build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass. Media still imports its `Checkbox` (now re-exported); nothing else uses `ui.tsx` yet.

- [ ] **Step 4: Commit**

```bash
git add src/components/ui.tsx src/screens/media/Checkbox.tsx
git commit -m "feat(ui): add shared Notice/LoadingState/EmptyState/EditorBar/Checkbox primitives"
```

---

## Task 6: StatusBadge everywhere (dot-everywhere)

**Files:**
- Modify: `ui/src/screens/ContentList.tsx` (lines ~268-275 status cell, ~298-310 local Checkbox, ~312-314 local initials, avatar at ~139)
- Modify: `ui/src/screens/EntryEditor.tsx` (lines ~145-149 status span)

- [ ] **Step 1: ContentList — use StatusBadge for the publish status cell**

In `ContentList.tsx`, `StatusBadge` is already imported from `../components/shell`. Replace the `{dp && (<td>…raw rs-status…</td>)}` block (the one rendering `<span className="rs-status rs-status--ok">Published</span>` / `--muted">Draft`) with:
```tsx
                {dp && (
                  <td>
                    <StatusBadge status={e.published_at ? "published" : "draft"} />
                  </td>
                )}
```

- [ ] **Step 2: ContentList — drop local Checkbox + initials, import shared**

At top of `ContentList.tsx`, change the shell import to also pull nothing new, and add:
```tsx
import { Checkbox } from "../components/ui";
import { initials, AVATAR_NEUTRAL } from "./../util";
```
(There is already `import { relTime, relationLabel, shortId } from "../util";` — merge `initials, AVATAR_NEUTRAL` into that existing import instead of adding a second line: `import { relTime, relationLabel, shortId, initials, AVATAR_NEUTRAL } from "../util";`. Import `Checkbox` from `../components/ui`.)

Then delete the local `function Checkbox(...)` (the `~298-310` block) and the local `function initials(...)` (`~312-314`) at the bottom of the file.

- [ ] **Step 3: ContentList — use AVATAR_NEUTRAL for the relation avatar**

In `renderCell`, the relation branch builds an `Avatar` with `color="#52525B"`. Change to `color={AVATAR_NEUTRAL}` and its `initials={initials(String(label))}` (already calls a local `initials`; now uses the imported one — same call site).

- [ ] **Step 4: EntryEditor — use StatusBadge in the editor bar**

In `EntryEditor.tsx`, add to imports: `import { StatusBadge } from "../components/shell";`. Replace the status span block:
```tsx
          {dp && !isNew && (
            <span className={"rs-status " + (isPublished ? "rs-status--ok" : "rs-status--muted")}>
              {isPublished ? "Published" : "Draft"}
            </span>
          )}
```
with:
```tsx
          {dp && !isNew && (
            <StatusBadge status={isPublished ? "published" : "draft"} />
          )}
```

- [ ] **Step 5: Verify no raw rs-status spans remain**

Run:
```bash
grep -rn 'rs-status rs-status--\|rs-status " +' src --include="*.tsx" | grep -v shell.tsx
```
Expected: zero matches.

- [ ] **Step 6: Verify build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass.

- [ ] **Step 7: Commit**

```bash
git add src/screens/ContentList.tsx src/screens/EntryEditor.tsx
git commit -m "refactor(ui): render all status pills via StatusBadge"
```

---

## Task 7: Migrate notices + editor bars + avatars across screens

**Files:**
- Modify: `ui/src/screens/EntryEditor.tsx`, `UserEditor.tsx`, `RoleDetail.tsx`, `MediaLibrary.tsx`, `MediaSettings.tsx`
- Modify: `ui/src/screens/media/AssetDetail.tsx`, `media/FolderModal.tsx`
- Modify: `ui/src/builder/CreateTypeModal.tsx`, `SaveConfirmModal.tsx`, `FieldConfigModal.tsx`, `SchemaEditor.tsx`
- Modify: `ui/src/screens/Login.tsx`
- Modify: `ui/src/screens/Users.tsx`, `Roles.tsx`, `RoleDetail.tsx` (avatar color)

- [ ] **Step 1: Swap every `rs-login-error` banner for `<Notice>`**

For each file below, replace the banner div with `<Notice>…</Notice>` (and `<Notice tone="ok">` for the success case). Add `import { Notice } from "../components/ui";` (use the correct relative depth: `../../components/ui` for files under `screens/media/`, `../components/ui` for `screens/` and `builder/`).

Concrete replacements:

`EntryEditor.tsx` line ~176: `{banner && <div className="rs-login-error" style={{ margin: "0 24px" }}>{banner}</div>}` →
```tsx
      {banner && <div style={{ margin: "0 24px" }}><Notice>{banner}</Notice></div>}
```
(Keep the page-edge inset wrapper since the editor body has no horizontal padding here; `Notice` supplies the bottom margin.)
And line ~221 `{error && <div className="rs-login-error">{error}</div>}` → `{error && <Notice>{error}</Notice>}`.

`UserEditor.tsx` line ~96: `{error && <div className="rs-login-error" style={{ margin: "0 24px" }}>{error}</div>}` →
```tsx
      {error && <div style={{ margin: "0 24px" }}><Notice>{error}</Notice></div>}
```

`MediaLibrary.tsx` line ~179: `{notice && <div className="rs-login-error" style={{ marginBottom: 12 }}>{notice}</div>}` → `{notice && <Notice>{notice}</Notice>}`.

`media/AssetDetail.tsx` line ~61: `{error && <div className="rs-login-error" style={{ marginBottom: 12 }}>{error}</div>}` → `{error && <Notice>{error}</Notice>}`.

`media/FolderModal.tsx` line ~49: same pattern → `{error && <Notice>{error}</Notice>}`.

`builder/CreateTypeModal.tsx` line ~42: `{err && <div className="rs-login-error" style={{ marginBottom: 12 }}>{err}</div>}` → `{err && <Notice>{err}</Notice>}`.

`builder/FieldConfigModal.tsx` line ~87: same → `{err && <Notice>{err}</Notice>}`.

`builder/SaveConfirmModal.tsx` line ~29: the `<div className="rs-login-error" style={{ marginBottom: 12 }}>…</div>` wrapping warning text → wrap its children in `<Notice>…</Notice>`.

`builder/SchemaEditor.tsx` lines ~150, ~151, ~153, ~160: each `rs-login-error` div → `<Notice>…</Notice>` (preserve the inner content of each).

`MediaSettings.tsx` lines ~141-142:
```tsx
          {status.kind === "ok" && <Notice tone="ok">{status.message}</Notice>}
          {status.kind === "error" && <Notice>{status.message}</Notice>}
```
(removes the `rs-settings-ok` class usage).

`Login.tsx` line ~139: `{error && <div className="rs-login-error">{error}</div>}` → `{error && <Notice>{error}</Notice>}`.

- [ ] **Step 2: Adopt `EditorBar` in the three bare editors**

`UserEditor.tsx` — replace the `<div className="rs-editor-bar">…</div>` block (back button + titlewrap + actions, lines ~77-94) with:
```tsx
      <EditorBar
        onBack={() => navigate("/users")}
        title={isNew ? "Add a user" : email || "User"}
        actions={
          <>
            {!isNew && (
              <button className="rs-btn rs-btn--ghost rs-danger" disabled={busy} onClick={remove}>
                <Icons.trash size={15} /> Delete
              </button>
            )}
            <button className="rs-btn rs-btn--primary" disabled={busy || !email || (isNew && !password)} onClick={save}>
              <Icons.check size={15} /> {isNew ? "Create user" : "Save user"}
            </button>
          </>
        }
      />
```
Add `import { EditorBar } from "../components/ui";` (merge with the Notice import). The `Icons` import already exists.

`RoleDetail.tsx` — both the not-found bar (lines ~20-27) and the main bar (lines ~48-61). Replace not-found bar:
```tsx
        <EditorBar onBack={() => navigate("/roles")} title="Role not found" />
```
Replace main bar:
```tsx
        <EditorBar
          onBack={() => navigate("/roles")}
          title={
            <span className="rs-role-name">
              <span className="rs-rolebar-dot" style={{ ["--chip" as string]: role.color }} />
              {role.name}
              <span className="rs-role-system">System</span>
            </span>
          }
        />
```

`EntryEditor.tsx` — replace its `<div className="rs-editor-bar">…</div>` (lines ~139-174) with:
```tsx
      <EditorBar
        onBack={onBack}
        title={isNew ? `Create ${ct.display_name}` : `Edit ${ct.display_name}`}
        status={dp && !isNew ? <StatusBadge status={isPublished ? "published" : "draft"} /> : undefined}
        actions={
          <>
            {dp && !isNew && (
              <button
                className={"rs-btn " + (isPublished ? "rs-btn--ghost" : "rs-btn--primary")}
                onClick={togglePublish}
                disabled={publishing}
              >
                {publishing ? "…" : isPublished ? "Unpublish" : "Publish"}
              </button>
            )}
            <button
              className={"rs-btn " + (dp && isNew ? "rs-btn--ghost" : "rs-btn--primary")}
              onClick={() => save(false)}
              disabled={saving}
            >
              {saving ? "Saving…" : isNew ? "Create" : "Save"}
            </button>
            {dp && isNew && (
              <button className="rs-btn rs-btn--primary" onClick={() => save(true)} disabled={saving}>
                {saving ? "…" : "Create & Publish"}
              </button>
            )}
          </>
        }
      />
```
Add `import { EditorBar, Notice } from "../components/ui";` (merge). `StatusBadge` import was added in Task 6.

- [ ] **Step 3: Replace `#52525B` avatars with `AVATAR_NEUTRAL` + `initials()`**

In `Users.tsx`, `Roles.tsx`, `RoleDetail.tsx`, `shell.tsx`: change each `<Avatar … color="#52525B" … />` to `color={AVATAR_NEUTRAL}`, and each `initials={u.email.slice(0, 2).toUpperCase()}` to `initials={initials(u.email)}`. Add `import { initials, AVATAR_NEUTRAL } from "../util";` (or `"./../util"`; for `shell.tsx` it is `"../util"`). In `shell.tsx` the two hard-coded `color="#52525B"` are the sidebar foot avatar and the topbar user avatar — replace both; their `initials` are literal (`"AD"`, `(email ?? "AD").slice(0,2)…`) — leave the `"AD"` fallbacks but you may switch the email one to `initials(email ?? "Admin")`.

- [ ] **Step 4: Verify no stale references remain**

Run:
```bash
grep -rn 'rs-login-error\|rs-settings-ok' src --include="*.tsx"
grep -rn '#52525B' src --include="*.tsx"
```
Expected: both print zero matches.

- [ ] **Step 5: Verify build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass.

- [ ] **Step 6: Commit**

```bash
git add src/screens src/builder src/components/shell.tsx
git commit -m "refactor(ui): adopt Notice/EditorBar/AVATAR_NEUTRAL across screens"
```

---

## Task 8: Unify loading / empty copy

**Files:**
- Modify: `ui/src/screens/ContentList.tsx`, `Dashboard.tsx`, `Users.tsx`, `Roles.tsx`, `RoleDetail.tsx`, `EntryEditor.tsx`

- [ ] **Step 1: Replace ad-hoc loading/empty divs with shared components**

Import `LoadingState` / `EmptyState` from `../components/ui` in each file and swap:
- `<div className="rs-empty">Loading…</div>` → `<LoadingState />`
- `<div className="rs-empty">Failed to load users.</div>` (Users) → `<EmptyState>Couldn’t load users.</EmptyState>`
- Keep retry-button empties as `<EmptyState>…<button className="rs-link-btn" …>Retry</button></EmptyState>` (wrap the existing children).
- `RoleDetail` not-found body `<div className="rs-empty">Role "…" does not exist…</div>` → `<EmptyState>…</EmptyState>`.

Do NOT change `MediaLibrary`'s `rs-media-empty` (distinct large empty-state component) or `TypePanel`'s skeletons. Copy convention: "Couldn’t load <noun>." for errors, "Loading…" for loads.

- [ ] **Step 2: Verify build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass.

- [ ] **Step 3: Commit**

```bash
git add src/screens
git commit -m "refactor(ui): unify loading and empty states"
```

---

## Task 9: Remove dead / duplicate CSS

**Files:**
- Modify: `ui/src/styles.css`

- [ ] **Step 1: Delete duplicate `rs-count-pill`**

`rs-count-pill` is defined at ~line 778 and again inside the `.rs-media-sectionhead .rs-count-pill` at ~790 — the latter is an exact duplicate of the former's properties scoped redundantly. Remove the redundant `.rs-media-sectionhead .rs-count-pill { … }` rule (the standalone `.rs-count-pill` at 778 already applies). Keep the `.rs-media-sectionhead .rs-spacer` rule.

- [ ] **Step 2: Delete duplicate `rs-radio-dot`**

Two identical-purpose definitions exist (~line 652 and ~746). Keep the one at ~652 (in the radio-cards section); delete the second standalone `.rs-radio-dot { … }` at ~746 (the `.rs-role-radio.is-on .rs-radio-dot` override below it stays).

- [ ] **Step 3: Confirm `rs-danger` is now single-sourced**

After Task 2 there should be one `.rs-danger { color: var(--danger); }` base rule plus its `:hover`. If a redundant bare `.rs-danger`/`.rs-danger:hover` pair remains (the ~764-765 block duplicating ~190 intent), keep ~764-765 as the base definition and ensure ~190 is only the ghost-border hover variant. Verify by eye; no functional dup should remain.

- [ ] **Step 4: Verify build + visual sanity for the touched components**

Run: `pnpm typecheck && pnpm build`
Expected: both pass. (Removing duplicate CSS rules cannot change rendering since they were identical.)

- [ ] **Step 5: Commit**

```bash
git add src/styles.css
git commit -m "refactor(ui): remove duplicate CSS rules"
```

---

## Task 10: Full verification + visual pass

**Files:** none (verification only).

- [ ] **Step 1: Grep guards all clean**

Run (from `ui/`):
```bash
grep -rn 'rs-login-error\|rs-settings-ok' src
grep -rn '#52525B\|#b91c1c\|#c0392b\|#1a7f4b' src
grep -rn 'rs-status rs-status--' src --include="*.tsx" | grep -v shell.tsx
```
Expected: all three print zero matches.

- [ ] **Step 2: Typecheck + build**

Run: `pnpm typecheck && pnpm build`
Expected: both pass.

- [ ] **Step 3: Visual walk (playwright skill)**

Use the `playwright-skill` to start the dev server (`pnpm dev`) and screenshot each route in **light and dark**: `/` (Dashboard), `/content/<type>` (list) and an entry editor, `/builder`, `/media` and `/settings/media`, `/users`, `/roles`, a role detail, a user editor, `/login`, `/settings`. Confirm: status pills identical (dot + label) on list, editor, and dashboard; notice banners consistent; back-bars identical across the three editors; avatars unchanged. Note any regression and fix before final commit.

- [ ] **Step 4: Final commit (if visual fixes were needed)**

```bash
git add -A src
git commit -m "fix(ui): visual-pass corrections"
```
(Skip if Step 3 found nothing.)

---

## Done criteria
- `pnpm typecheck` and `pnpm build` green.
- All grep guards in Task 10 Step 1 return nothing.
- Every status pill renders via `StatusBadge`; every banner via `Notice`; the three bare editors use `EditorBar`; one `Checkbox`; `initials()`/`AVATAR_NEUTRAL` used at all avatar sites.
- No layout or behavior change observed in the visual walk.
