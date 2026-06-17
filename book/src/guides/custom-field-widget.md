# Add a custom field widget

The admin UI renders every field in an entry editor through one component:
`ui/src/components/FieldInput.tsx`. It switches on the field's
[`kind`](../concepts/fields.md) and returns the input for that kind — a textarea
for `text`, a toggle for `boolean`, a select for `enum`, and so on. To change
how a field is edited, you add or replace a `case` in that switch.

This guide adds a custom widget: a color picker for a `text` field that stores a
hex string. Read [Admin UI architecture](../concepts/admin-ui.md) first if you
have not seen the layout.

## How `FieldInput` works

`FieldInput` receives the field definition, the current `value`, and an
`onChange` callback. Its job is to render a control and call `onChange` with the
new value. The switch looks like this (trimmed):

```tsx
export function FieldInput({ field, value, onChange }: {
  field: Field;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const str = typeof value === "string" ? value : value == null ? "" : String(value);
  switch (field.kind) {
    case "text":
    case "json":
      return <textarea className="rs-input rs-textarea" /* … */ />;
    case "boolean":
      return <button className="rs-toggle" onClick={() => onChange(!value)} /* … */ />;
    // …
    default:
      return <input className="rs-input" value={str} onChange={(e) => onChange(e.target.value)} />;
  }
}
```

Two contracts matter:

- **Read from `value`, write through `onChange`.** The component is controlled.
  Never hold the field value in your own state as the source of truth.
- **Style with `rs-` tokens.** Use `rs-input` and the design tokens (see
  `DESIGN.md`) so your widget matches the rest of the editor and themes
  correctly.

## Decide what triggers the widget

A widget can key off anything on the `Field`. The simplest dispatch is by
`kind` — but that replaces the input for *every* field of that kind. To opt in
per field, branch on field metadata instead. Here we render a color picker only
when a `text` field carries a `format: "color"` hint in its `kind_meta`, and
fall back to the normal textarea otherwise.

## Add the widget

Edit `ui/src/components/FieldInput.tsx`. Replace the `text`/`json` case so a
color-formatted text field gets the picker:

```tsx
case "text":
case "json": {
  // Opt-in: a text field flagged as a color renders the picker.
  if (field.kind === "text" && field.kind_meta?.format === "color") {
    return <ColorField value={str} onChange={onChange} />;
  }
  return (
    <textarea
      className="rs-input rs-textarea"
      rows={field.kind === "json" ? 6 : 3}
      value={typeof value === "object" && value !== null ? JSON.stringify(value, null, 2) : str}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}
```

Then define the widget alongside the other field components in the same file:

```tsx
function ColorField({ value, onChange }: {
  value: string;
  onChange: (v: unknown) => void;
}) {
  const hex = /^#[0-9a-fA-F]{6}$/.test(value) ? value : "#000000";
  return (
    <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
      <input
        type="color"
        value={hex}
        onChange={(e) => onChange(e.target.value)}
        aria-label="Pick a color"
      />
      <input
        className="rs-input"
        value={value}
        placeholder="#RRGGBB"
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}
```

The native color input and the text input both write through the same
`onChange`, so they stay in sync and the entry editor persists the value like
any other `text` field.

## Flag a field to use it

The widget only appears when a field carries the `format: "color"` hint. Add
that to a content type's field definition. If you manage schema as code, set it
in the field's metadata; see [Schema as code](schema-as-code.md) for the file
format. The field's `kind` stays `text` — the value is still stored and served
as a plain string, so your API consumers are unaffected.

## Verify it

Type-check and run the dev server, then open an entry that has the flagged
field:

```sh
cd ui
pnpm typecheck
pnpm dev      # http://localhost:5173
```

Confirm the color picker renders, that picking a color and typing a hex both
update the field, and that saving the entry round-trips the value. When it
works, [build and embed the bundle](build-admin-ui.md) so the server serves your
change at `/studio`.
