# Field Type Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the design's two-step add-field flow in the Content-Type Builder — a card-grid field-type picker before the field config modal.

**Architecture:** Add a typed `FieldPicker` card-grid modal (reusing existing `.rs-fieldgrid` CSS). `SchemaEditor` drives a 3-state modal (`null | pick | config`): adding a field opens the picker, picking a kind opens config seeded with that kind, editing skips the picker. The type `<select>` is removed from `FieldConfigModal`, which gains a "Back" button when adding.

**Tech Stack:** React 18 + TypeScript, Vite, react-router-dom. No test runner in `ui/` — verification is `tsc` typecheck + `vite build` + manual check. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-06-04-field-type-picker-design.md`

---

### Task 1: Add `mail` and `braces` icons

**Files:**
- Modify: `ui/src/components/icons.tsx` (insert before the closing `} as const;` at line 136)

- [ ] **Step 1: Add the two icon entries**

In `ui/src/components/icons.tsx`, add these entries to the `Icons` object, immediately after the `upload` entry (line 133-135) and before the closing `} as const;`:

```tsx
  mail: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/></g>} />
  ),
  braces: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M8 4a3 3 0 0 0-3 3v2a2 2 0 0 1-2 2 2 2 0 0 1 2 2v2a3 3 0 0 0 3 3"/><path d="M16 4a3 3 0 0 1 3 3v2a2 2 0 0 0 2 2 2 2 0 0 0-2 2v2a3 3 0 0 1-3 3"/></g>} />
  ),
```

- [ ] **Step 2: Point FieldRow at the new icons**

In `ui/src/builder/FieldRow.tsx`, update `KIND_ICON` (lines 7-22): change `json: "doc"` to `json: "braces"` and `email: "type"` to `email: "mail"`. Also remove the now-stale comment on lines 5-6 ("icons.tsx has no braces/mail keys, so json/email fall back to doc/type.") — replace those two comment lines with: `// kind → icon key in ui/src/components/icons.tsx.`

- [ ] **Step 3: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS (no errors).

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/icons.tsx ui/src/builder/FieldRow.tsx
git commit -m "feat(builder): add mail and braces icons for field rows"
```

---

### Task 2: Add field-card catalog + `blankField(kind)` to draftModel

**Files:**
- Modify: `ui/src/builder/draftModel.ts:1-9` (imports + add catalog), `:37-53` (`blankField`)

- [ ] **Step 1: Import the icon key type**

In `ui/src/builder/draftModel.ts`, add to the top imports (after line 4):

```ts
import type { IconKey } from "../components/icons";
```

- [ ] **Step 2: Add the FIELD_CARDS catalog and fieldLabel helper**

In `ui/src/builder/draftModel.ts`, immediately after the `KINDS` array (after line 9), add:

```ts
/** Picker cards — one per user-addable FieldKind, with a friendly label.
 *  `uuid` is server-managed and intentionally excluded. */
export const FIELD_CARDS: { kind: FieldKind; label: string; desc: string; icon: IconKey }[] = [
  { kind: "string",   label: "Short text",  desc: "Small text like a title or name",       icon: "type" },
  { kind: "text",     label: "Long text",   desc: "Multi-line text or description",         icon: "doc" },
  { kind: "email",    label: "Email",       desc: "An email with built-in validation",      icon: "mail" },
  { kind: "slug",     label: "Slug",        desc: "A URL-friendly identifier",              icon: "hash" },
  { kind: "url",      label: "URL",         desc: "A web address",                          icon: "link" },
  { kind: "integer",  label: "Integer",     desc: "Whole numbers",                          icon: "hash" },
  { kind: "float",    label: "Decimal",     desc: "Decimals and floats",                    icon: "hash" },
  { kind: "boolean",  label: "Boolean",     desc: "A yes-or-no toggle",                     icon: "toggle" },
  { kind: "datetime", label: "Datetime",    desc: "A date, time or date-time",              icon: "calendar" },
  { kind: "enum",     label: "Enumeration", desc: "A list of values to pick from",          icon: "layers" },
  { kind: "relation", label: "Relation",    desc: "Link entries across types",              icon: "relation" },
  { kind: "media",    label: "Media",       desc: "Files — images, video, audio, documents", icon: "image" },
  { kind: "json",     label: "JSON",        desc: "Raw, structured JSON data",              icon: "braces" },
];

/** Friendly label for a kind; falls back to the raw kind string. */
export function fieldLabel(kind: FieldKind): string {
  return FIELD_CARDS.find((c) => c.kind === kind)?.label ?? kind;
}
```

- [ ] **Step 3: Let blankField accept a starting kind**

In `ui/src/builder/draftModel.ts`, change the `blankField` signature (line 37) from:

```ts
export function blankField(): DraftField {
  return {
    id: crypto.randomUUID(),
    name: "",
    kind: "string",
```

to:

```ts
export function blankField(kind: FieldKind = "string"): DraftField {
  return {
    id: crypto.randomUUID(),
    name: "",
    kind,
```

(Leave the rest of the returned object unchanged.)

- [ ] **Step 4: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/builder/draftModel.ts
git commit -m "feat(builder): add field-card catalog and kind-seeded blankField"
```

---

### Task 3: Create the FieldPicker component

**Files:**
- Create: `ui/src/builder/FieldPicker.tsx`

- [ ] **Step 1: Write the component**

Create `ui/src/builder/FieldPicker.tsx` with this exact content:

```tsx
import { useEffect, useRef } from "react";
import { Icons } from "../components/icons";
import type { FieldKind } from "../api/types";
import { FIELD_CARDS } from "./draftModel";

export function FieldPicker({
  typeDisplay,
  isFirst,
  onPick,
  onClose,
}: {
  typeDisplay: string;
  isFirst: boolean;   // no fields yet → "Add your first field"
  onPick: (kind: FieldKind) => void;
  onClose: () => void;
}) {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onCloseRef.current(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div
        className="rs-modal rs-modal--wide"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="rs-modal-head">
          <div className="rs-modal-icon"><Icons.layers size={18} /></div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">{typeDisplay}</span>
            <h2>{isFirst ? "Add your first field" : "Select a field type"}</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose}><Icons.x size={18} /></button>
        </div>

        <div className="rs-modal-body">
          <div className="rs-fieldgrid">
            {FIELD_CARDS.map((ft) => {
              const I = Icons[ft.icon];
              return (
                <button
                  key={ft.kind}
                  className="rs-fieldgrid-item"
                  onClick={() => onPick(ft.kind)}
                >
                  <div className="rs-fieldgrid-icon"><I size={20} /></div>
                  <div className="rs-fieldgrid-text">
                    <strong>{ft.label}</strong>
                    <span>{ft.desc}</span>
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS. (Component is not yet imported anywhere — that's fine; it must still typecheck.)

- [ ] **Step 3: Commit**

```bash
git add ui/src/builder/FieldPicker.tsx
git commit -m "feat(builder): add FieldPicker card-grid modal"
```

---

### Task 4: Remove type dropdown from FieldConfigModal, add Back button

**Files:**
- Modify: `ui/src/builder/FieldConfigModal.tsx:13-27` (props), `:65-72` (header eyebrow), `:99-112` (remove select), `:238-244` (footer)

- [ ] **Step 1: Add onBack prop and import fieldLabel**

In `ui/src/builder/FieldConfigModal.tsx`, line 4, change:

```ts
import { KINDS, type Cardinality, type DraftField } from "./draftModel";
```

to:

```ts
import { fieldLabel, type Cardinality, type DraftField } from "./draftModel";
```

(`KINDS` is no longer used here once the select is gone.)

Then add `onBack` to the props. Change the props block (lines 13-27) — add `onBack` after `onSave`:

```tsx
export function FieldConfigModal({
  initial,
  isNew,
  typeNames,
  lockedEnumValues,
  onSave,
  onBack,
  onClose,
}: {
  initial: DraftField;
  isNew: boolean;             // adding a brand-new field (vs editing existing row)
  typeNames: string[];
  lockedEnumValues: string[]; // existing enum values that cannot be removed
  onSave: (field: DraftField) => void;
  onBack?: () => void;        // adding a new field → return to the type picker
  onClose: () => void;
}) {
```

- [ ] **Step 2: Show the field-type label in the header eyebrow**

In `ui/src/builder/FieldConfigModal.tsx`, the header eyebrow currently reads (line 68):

```tsx
            <span className="rs-modal-eyebrow">{isNew ? "Add a field" : "Edit field"}</span>
```

Change it to include the friendly type label:

```tsx
            <span className="rs-modal-eyebrow">
              {(isNew ? "Add a field" : "Edit field")} · {fieldLabel(field.kind)}
            </span>
```

- [ ] **Step 3: Remove the Type select block**

In `ui/src/builder/FieldConfigModal.tsx`, delete the entire Type `<div className="rs-field">` block (lines 99-112), which is:

```tsx
              <div className="rs-field">
                <div className="rs-field-label">
                  <label>Type</label>
                  {locked && <span className="rs-field-hint">type can't be changed after creation</span>}
                </div>
                <select
                  className="rs-input"
                  value={field.kind}
                  disabled={locked}
                  onChange={(e) => set({ kind: e.target.value as FieldKind })}
                >
                  {KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
                </select>
              </div>
```

Remove it entirely. (The Name field above it and the `field.kind === "relation"` block below it stay.)

- [ ] **Step 4: Remove the now-unused FieldKind import**

After Step 3, `FieldKind` is no longer referenced in this file. In `ui/src/builder/FieldConfigModal.tsx` line 3, delete:

```ts
import type { FieldKind } from "../api/types";
```

- [ ] **Step 5: Add the Back button to the footer**

In `ui/src/builder/FieldConfigModal.tsx`, the footer (lines 238-244) currently is:

```tsx
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <div className="rs-spacer" />
          <button className="rs-btn rs-btn--primary" onClick={save}>
            <Icons.check size={15} /> {isNew ? "Add field" : "Save changes"}
          </button>
        </div>
```

Change it to add a Back button (shown only when `onBack` is provided):

```tsx
        <div className="rs-modal-foot">
          {onBack && (
            <button className="rs-btn rs-btn--ghost" onClick={onBack}>
              <Icons.chevLeft size={15} /> Back
            </button>
          )}
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <div className="rs-spacer" />
          <button className="rs-btn rs-btn--primary" onClick={save}>
            <Icons.check size={15} /> {isNew ? "Add field" : "Save changes"}
          </button>
        </div>
```

- [ ] **Step 6: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS. (No unused-import or missing-symbol errors.)

- [ ] **Step 7: Commit**

```bash
git add ui/src/builder/FieldConfigModal.tsx
git commit -m "feat(builder): drop type select, add Back button to field config"
```

---

### Task 5: Wire the two-step flow into SchemaEditor

**Files:**
- Modify: `ui/src/builder/SchemaEditor.tsx:11-13` (imports), `:58-59` (modal state), `:88-90` (add/edit handlers), `:173-182` (render)

- [ ] **Step 1: Import FieldPicker**

In `ui/src/builder/SchemaEditor.tsx`, after the `FieldConfigModal` import (line 13), add:

```ts
import { FieldPicker } from "./FieldPicker";
```

Also update the draftModel import (line 11) to bring in `FieldKind` indirectly — it is already typed through `blankField`, so no change needed there. Confirm line 11 still reads:

```ts
import { blankField, type DraftField } from "./draftModel";
```

Add the `FieldKind` type import near the other api/type imports (after line 9 `import { enumValues } from "../api/types";`):

```ts
import type { FieldKind } from "../api/types";
```

- [ ] **Step 2: Replace the modal state type**

In `ui/src/builder/SchemaEditor.tsx`, replace lines 58-59:

```tsx
  // Field edit modal: { field, isNew } when open, null when closed.
  const [modal, setModal] = useState<{ field: DraftField; isNew: boolean } | null>(null);
```

with a 3-state union:

```tsx
  // Field modal: "pick" = choosing a type, "config" = editing one, null = closed.
  type FieldModal =
    | { step: "pick" }
    | { step: "config"; field: DraftField; isNew: boolean };
  const [modal, setModal] = useState<FieldModal | null>(null);
```

- [ ] **Step 3: Update the add/edit handlers**

In `ui/src/builder/SchemaEditor.tsx`, replace lines 88-90:

```tsx
  const addField = () => setModal({ field: blankField(), isNew: true });

  const editField = (f: DraftField) => setModal({ field: f, isNew: false });
```

with:

```tsx
  const addField = () => setModal({ step: "pick" });

  const pickKind = (kind: FieldKind) =>
    setModal({ step: "config", field: blankField(kind), isNew: true });

  const editField = (f: DraftField) =>
    setModal({ step: "config", field: f, isNew: false });
```

- [ ] **Step 4: Update the render block**

In `ui/src/builder/SchemaEditor.tsx`, replace the modal render (lines 173-182):

```tsx
      {modal && (
        <FieldConfigModal
          initial={modal.field}
          isNew={modal.isNew}
          typeNames={allTypes.data?.map((t) => t.name) ?? []}
          lockedEnumValues={lockedEnum(modal.field)}
          onSave={saveField}
          onClose={() => setModal(null)}
        />
      )}
```

with:

```tsx
      {modal?.step === "pick" && (
        <FieldPicker
          typeDisplay={draft.display_name || draft.name}
          isFirst={draft.fields.length === 0}
          onPick={pickKind}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.step === "config" && (
        <FieldConfigModal
          initial={modal.field}
          isNew={modal.isNew}
          typeNames={allTypes.data?.map((t) => t.name) ?? []}
          lockedEnumValues={lockedEnum(modal.field)}
          onSave={saveField}
          onBack={modal.isNew ? () => setModal({ step: "pick" }) : undefined}
          onClose={() => setModal(null)}
        />
      )}
```

- [ ] **Step 5: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS. Watch for: `lockedEnum(modal.field)` — `modal.field` is only valid inside the `step === "config"` branch, which it is. No errors expected.

- [ ] **Step 6: Build**

Run: `cd ui && pnpm build`
Expected: `tsc -b` clean, `vite build` succeeds (writes `dist/`).

- [ ] **Step 7: Commit**

```bash
git add ui/src/builder/SchemaEditor.tsx
git commit -m "feat(builder): two-step add-field flow via type picker"
```

---

### Task 6: Manual verification

**Files:** none (verification only)

- [ ] **Step 1: Run the dev server**

Run: `cd ui && pnpm dev`
Open the printed local URL, log in, go to the Content-Type Builder, open or create a content type.

- [ ] **Step 2: Verify the add flow**

Checklist:
- Click "Add another field" → picker modal appears titled "Select a field type" with 13 cards (Short text, Long text, Email, Slug, URL, Integer, Decimal, Boolean, Datetime, Enumeration, Relation, Media, JSON).
- Pick a card (e.g. Relation) → config modal opens; eyebrow reads "Add a field · Relation"; relation-specific fields show.
- Click "← Back" → returns to the picker.
- Pick another card → config opens → fill name → "Add field" → row appears in the schema with the right icon/pill.

- [ ] **Step 3: Verify the edit flow + first-field title**

- Click the edit (pencil) on an existing field → goes straight to config, no picker; no Back button; type cannot be changed (no type select present).
- On a brand-new type with zero fields, "Add field" picker title reads "Add your first field".
- `email` field row shows the mail icon; `json` row shows the braces icon.

- [ ] **Step 4: Stop the dev server** (Ctrl-C).

No commit (verification only). If any check fails, fix in the relevant task's files and re-verify.

---

## Self-Review notes

- **Spec coverage:** friendly-label map → Task 2; FieldPicker → Task 3; SchemaEditor 3-state flow → Task 5; FieldConfigModal select removal + Back → Task 4; mail/braces icons → Task 1; testing → Task 6. All spec sections covered.
- **Type consistency:** `blankField(kind)`, `FIELD_CARDS`, `fieldLabel`, `FieldModal` union, `onBack?` — names used identically across tasks.
- **uuid exclusion:** documented in catalog comment (Task 2), matches spec.
