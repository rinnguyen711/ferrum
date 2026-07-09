# Field type picker — design

## Problem

The Content-Type Builder design (`design/ferrum/modals.jsx`) specifies a two-step
add-field flow:

1. **FieldPicker** — a card-grid modal ("Select a field type") where the user picks
   a kind from labelled cards.
2. **FieldConfigModal** — configure the chosen kind.

The shipped UI dropped step 1. `addField` in
`ui/src/builder/SchemaEditor.tsx:88` jumps straight to `FieldConfigModal` with a
blank field, and the kind is chosen via a plain `<select>` dropdown inside config.
No `FieldPicker` component exists in `ui/`.

The picker's CSS (`.rs-fieldgrid`, 9 rules) is already present in
`ui/src/styles.css` — only the component was never ported.

## Goal

Restore the design's two-step flow for **adding** a new field. Editing an existing
field keeps going straight to config (type is locked after creation).

No backend change. No new dependencies. Reuse existing `.rs-fieldgrid` CSS.

## Friendly-label map

The design's 12 cards use display labels (Text, Rich text, Number, UID,
Component…) that do not map 1:1 to the real `FieldKind` enum. The real set
(`KIND_ICON` in `ui/src/builder/FieldRow.tsx`) is 13 kinds:

```
string, text, slug, integer, float, boolean, datetime,
uuid, relation, media, enum, json, email, url
```

Decision (user-confirmed): **one card per real `FieldKind`**, each with a friendly
label, description, and icon. Honest to the backend; keeps the design's card look.

| FieldKind  | Card label   | desc                                         | icon     |
|------------|--------------|----------------------------------------------|----------|
| string     | Short text   | Small text like a title or name              | type     |
| text       | Long text    | Multi-line text or description               | doc      |
| email      | Email        | An email with built-in validation            | mail     |
| slug       | Slug         | A URL-friendly identifier                    | hash     |
| url        | URL          | A web address                                | link     |
| integer    | Integer      | Whole numbers                                | hash     |
| float      | Decimal      | Decimals and floats                          | hash     |
| boolean    | Boolean      | A yes-or-no toggle                           | toggle   |
| datetime   | Datetime     | A date, time or date-time                    | calendar |
| enum       | Enumeration  | A list of values to pick from                | layers   |
| relation   | Relation     | Link entries across types                    | relation |
| media      | Media        | Files — images, video, audio, documents      | image    |
| json       | JSON         | Raw, structured JSON data                    | braces   |

`uuid` is server-managed (not user-addable today — already absent from the type
dropdown's practical use); it is **excluded** from picker cards. If a `uuid` field
exists it still renders fine in `FieldRow`.

Note: `mail` and `braces` icons are missing from `ui/src/components/icons.tsx`
(today `email`/`json` fall back to `type`/`doc`). Add both icons so picker cards
and `FieldRow` show the right glyph.

The label map lives in `draftModel.ts` (single source of truth) so `FieldRow` can
reuse the friendly label for its type pill if desired (out of scope — pill stays
as-is unless trivial).

## Components

### `FieldPicker.tsx` (new)

Card-grid modal. Mirrors `design/ferrum/modals.jsx:171-190`, typed, real kinds.

```
FieldPicker({ typeDisplay, isFirst, onPick, onClose })
  - renders .rs-fieldgrid of buttons, one per FIELD_CARDS entry
  - title: isFirst ? "Add your first field" : "Select a field type"
  - onPick(kind: FieldKind)
```

### `SchemaEditor.tsx` (modified)

Modal state goes from `{ field, isNew } | null` to a 3-state union:

```
type FieldModal =
  | { step: "pick" }
  | { step: "config"; field: DraftField; isNew: boolean }
  | null;
```

- `addField` → `{ step: "pick" }`
- picker `onPick(kind)` → `{ step: "config", field: blankField(kind), isNew: true }`
- `editField(f)` → `{ step: "config", field: f, isNew: false }` (no picker)
- picker `onClose` / config `onClose` → `null`
- config `onBack` (new field only) → `{ step: "pick" }`

`isFirst` passed to picker = `draft.fields.length === 0`.

### `FieldConfigModal.tsx` (modified)

- Remove the Type `<select>` block (lines 99-112).
- Add optional `onBack?: () => void`. When present, footer shows a "← Back" button
  (left of Cancel) that calls `onBack`. Edit mode passes no `onBack` → no button.
- Header eyebrow shows the chosen kind's friendly label.

### `draftModel.ts` (modified)

- Add `FIELD_CARDS: { kind: FieldKind; label: string; desc: string; icon: keyof typeof Icons }[]`
  (the table above). Export `fieldLabel(kind)` helper.
- `blankField(kind: FieldKind = "string")` — accept optional starting kind.

### `icons.tsx` (modified)

- Add `mail` and `braces` icon entries.

## Data flow

```
[+ Add field] click
  → SchemaEditor.addField → modal = {step:"pick"}
  → <FieldPicker onPick={kind => modal = {step:"config", field: blankField(kind), isNew:true}}>
  → <FieldConfigModal initial=field isNew onBack={() => modal = {step:"pick"}} onSave onClose>
  → onSave(field) → draft.fields updated, modal = null
```

Editing:
```
FieldRow edit → editField(f) → modal = {step:"config", field:f, isNew:false}  (no onBack)
```

## Error handling

Unchanged. Validation stays in `FieldConfigModal.save` (name required, etc.).
Picker has no validation — every card is a valid kind.

## Testing

- Build + typecheck pass (`vite build` / `tsc`).
- Manual / playwright: Add field → picker appears with 13 cards → pick → config
  opens for that kind → Back returns to picker → pick another → save → row appears.
- Edit existing field → goes straight to config, no picker, type not changeable.
- First field on a new type shows "Add your first field" title.
```
