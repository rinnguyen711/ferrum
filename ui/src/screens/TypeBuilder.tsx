import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { useResource } from "../hooks/useResource";
import { createContentType, listContentTypes } from "../api/endpoints";
import type { Field, FieldKind, NewContentType } from "../api/types";
import { ApiError } from "../api/client";

const KINDS: FieldKind[] = [
  "string", "text", "integer", "float", "boolean", "datetime",
  "relation", "enum", "json", "email", "url", "slug",
];

interface DraftField {
  id: string;
  name: string;
  kind: FieldKind;
  required: boolean;
  unique: boolean;
  enumValues: string[];
  target: string;
  inverse: string;
}

function blankField(): DraftField {
  return {
    id: crypto.randomUUID(),
    name: "",
    kind: "string",
    required: false,
    unique: false,
    enumValues: [],
    target: "",
    inverse: "",
  };
}

function toNewContentType(
  name: string,
  displayName: string,
  drafts: DraftField[],
): NewContentType {
  const fields: Field[] = drafts.map((d) => {
    let kind_meta: Record<string, unknown> = {};
    if (d.kind === "relation") {
      kind_meta = {
        target: d.target,
        cardinality: "many_to_one",
        ...(d.inverse ? { inverse: d.inverse } : {}),
      };
    } else if (d.kind === "enum") {
      kind_meta = { values: d.enumValues };
    }
    return {
      name: d.name,
      kind: d.kind,
      required: d.required,
      unique: d.unique,
      default: null,
      kind_meta,
    };
  });
  return { name, display_name: displayName, fields };
}

export function TypeBuilder() {
  const navigate = useNavigate();
  const allTypes = useResource(() => listContentTypes(), []);

  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [fields, setFields] = useState<DraftField[]>([blankField()]);
  const [saving, setSaving] = useState(false);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [banner, setBanner] = useState<string | null>(null);

  const updateField = (id: string, patch: Partial<DraftField>) =>
    setFields((fs) => fs.map((f) => (f.id === id ? { ...f, ...patch } : f)));
  const removeField = (id: string) =>
    setFields((fs) => fs.filter((f) => f.id !== id));
  const addField = () => setFields((fs) => [...fs, blankField()]);

  const clientError = (): string | null => {
    if (!name.trim()) return "Type name is required.";
    if (!displayName.trim()) return "Display name is required.";
    if (fields.length === 0) return "Add at least one field.";
    for (const f of fields) {
      if (!f.name.trim()) return "Every field needs a name.";
      if (f.kind === "enum" && f.enumValues.length === 0)
        return `Enum field “${f.name}” needs at least one value.`;
      if (f.kind === "relation" && !f.target)
        return `Relation field “${f.name}” needs a target type.`;
    }
    return null;
  };

  const save = async () => {
    setBanner(null);
    setFieldErrors({});
    const ce = clientError();
    if (ce) {
      setBanner(ce);
      return;
    }
    setSaving(true);
    try {
      await createContentType(toNewContentType(name, displayName, fields));
      navigate(`/builder/${name}`);
    } catch (e) {
      if (e instanceof ApiError) {
        if (e.fieldErrors.length) {
          const map: Record<string, string> = {};
          for (const fe of e.fieldErrors) map[fe.field] = fe.message ?? "Invalid";
          setFieldErrors(map);
          setBanner(e.message);
        } else {
          setBanner(e.message);
        }
      } else {
        setBanner("Create failed.");
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>New content type</h1>
          <p className="rs-cm-sub">Define a type and its fields.</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={save} disabled={saving}>
          {saving ? "Creating…" : "Create type"}
        </button>
      </div>

      {banner && <div className="rs-login-error" style={{ marginBottom: 12 }}>{banner}</div>}

      <div className="rs-fields">
        <div className="rs-field">
          <div className="rs-field-label">
            <label>name</label>
            <span className="rs-field-hint">lowercase letters, digits, underscore</span>
          </div>
          <input
            className="rs-input rs-mono"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="article"
          />
        </div>
        <div className="rs-field">
          <div className="rs-field-label">
            <label>display name</label>
          </div>
          <input
            className="rs-input"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="Article"
          />
        </div>
      </div>

      <h2 className="rs-cm-sub" style={{ marginTop: 20 }}>Fields</h2>
      <div className="rs-fieldrows">
        {fields.map((f) => (
          <FieldRow
            key={f.id}
            field={f}
            error={fieldErrors[f.name]}
            typeNames={allTypes.data?.map((t) => t.name) ?? []}
            onChange={(patch) => updateField(f.id, patch)}
            onRemove={() => removeField(f.id)}
          />
        ))}
      </div>
      <button className="rs-btn rs-btn--ghost" onClick={addField} style={{ marginTop: 12 }}>
        <Icons.plus size={15} /> Add field
      </button>
    </div>
  );
}

function FieldRow({
  field,
  error,
  typeNames,
  onChange,
  onRemove,
}: {
  field: DraftField;
  error?: string;
  typeNames: string[];
  onChange: (patch: Partial<DraftField>) => void;
  onRemove: () => void;
}) {
  return (
    <div className="rs-fieldrow">
      <div className="rs-fieldrow-main">
        <input
          className="rs-input rs-mono"
          placeholder="field_name"
          value={field.name}
          onChange={(e) => onChange({ name: e.target.value })}
        />
        <select
          className="rs-input"
          value={field.kind}
          onChange={(e) => onChange({ kind: e.target.value as FieldKind })}
        >
          {KINDS.map((k) => (
            <option key={k} value={k}>{k}</option>
          ))}
        </select>
        <label className="rs-checkbox">
          <input
            type="checkbox"
            checked={field.required}
            onChange={(e) => onChange({ required: e.target.checked })}
          />
          required
        </label>
        <label className="rs-checkbox">
          <input
            type="checkbox"
            checked={field.unique}
            onChange={(e) => onChange({ unique: e.target.checked })}
          />
          unique
        </label>
        <button className="rs-row-btn rs-danger" onClick={onRemove} title="Remove field">
          <Icons.trash size={15} />
        </button>
      </div>

      {field.kind === "relation" && (
        <div className="rs-fieldrow-sub">
          <select
            className="rs-input"
            value={field.target}
            onChange={(e) => onChange({ target: e.target.value })}
          >
            <option value="">target type…</option>
            {typeNames.map((n) => (
              <option key={n} value={n}>{n}</option>
            ))}
          </select>
          <input
            className="rs-input rs-mono"
            placeholder="inverse (optional)"
            value={field.inverse}
            onChange={(e) => onChange({ inverse: e.target.value })}
          />
        </div>
      )}

      {field.kind === "enum" && (
        <EnumEditor
          values={field.enumValues}
          onChange={(enumValues) => onChange({ enumValues })}
        />
      )}

      {error && <div className="rs-login-error">{error}</div>}
    </div>
  );
}

function EnumEditor({
  values,
  onChange,
}: {
  values: string[];
  onChange: (values: string[]) => void;
}) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const v = draft.trim();
    if (v && !values.includes(v)) onChange([...values, v]);
    setDraft("");
  };
  return (
    <div className="rs-fieldrow-sub">
      <div className="rs-chips rs-chips--wrap">
        {values.map((v) => (
          <span key={v} className="rs-chip">
            {v}
            <button
              className="rs-chip-x"
              onClick={() => onChange(values.filter((x) => x !== v))}
            >
              ×
            </button>
          </span>
        ))}
      </div>
      <div className="rs-input-affix">
        <input
          className="rs-input rs-mono"
          placeholder="enum value"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={add}>
          Add
        </button>
      </div>
    </div>
  );
}
