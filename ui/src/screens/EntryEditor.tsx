import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { useResource } from "../hooks/useResource";
import {
  createEntry,
  getContentType,
  getEntry,
  listEntries,
  updateEntry,
} from "../api/endpoints";
import type { Entry, Field } from "../api/types";
import { enumValues, relationMeta } from "../api/types";
import { relationLabel } from "../util";
import { ApiError } from "../api/client";

export function EntryEditor() {
  const { type = "", id = "new" } = useParams<{ type: string; id: string }>();
  const navigate = useNavigate();
  const isNew = id === "new";
  const onBack = () => navigate(`/content/${type}`);

  const schema = useResource(() => getContentType(type), [type]);
  const existing = useResource(
    () => (isNew ? Promise.resolve(null) : getEntry(type, id)),
    [type, id, isNew],
  );

  const [form, setForm] = useState<Record<string, unknown>>({});
  const [saving, setSaving] = useState(false);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [banner, setBanner] = useState<string | null>(null);

  // Seed the form once data is available.
  useEffect(() => {
    if (schema.data && (isNew || existing.data)) {
      const seed: Record<string, unknown> = {};
      for (const f of schema.data.fields) {
        seed[f.name] = existing.data ? existing.data[f.name] ?? "" : "";
      }
      setForm(seed);
    }
  }, [schema.data, existing.data, isNew]);

  if (schema.loading || existing.loading) return <div className="rs-empty">Loading…</div>;
  if (schema.error) return <div className="rs-empty">Couldn’t load type. {schema.error.message}</div>;
  if (existing.error) return <div className="rs-empty">{existing.error.message}</div>;
  const ct = schema.data;
  if (!ct) return <div className="rs-empty">Unknown content type.</div>;

  const set = (name: string, value: unknown) =>
    setForm((f) => ({ ...f, [name]: value }));

  const save = async () => {
    setSaving(true);
    setFieldErrors({});
    setBanner(null);
    // Build a body: omit empty strings (treated as "no value"); coerce numbers.
    const body: Record<string, unknown> = {};
    for (const f of ct.fields) {
      const v = form[f.name];
      if (v === "" || v === undefined) continue;
      if (f.kind === "integer" || f.kind === "float") {
        body[f.name] = Number(v);
      } else if (f.kind === "json") {
        try {
          body[f.name] = typeof v === "string" ? JSON.parse(v) : v;
        } catch {
          setFieldErrors((e) => ({ ...e, [f.name]: "Invalid JSON" }));
          setSaving(false);
          return;
        }
      } else {
        body[f.name] = v;
      }
    }
    try {
      if (isNew) await createEntry(type, body);
      else await updateEntry(type, id, body);
      navigate(`/content/${type}`);
    } catch (e) {
      if (e instanceof ApiError) {
        if (e.fieldErrors.length) {
          const map: Record<string, string> = {};
          for (const fe of e.fieldErrors) map[fe.field] = fe.message ?? "Invalid";
          setFieldErrors(map);
        } else {
          setBanner(e.message);
        }
      } else {
        setBanner("Save failed.");
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={onBack}>
          <Icons.arrowLeft size={18} />
        </button>
        <div className="rs-editor-titlewrap">
          <h1>{isNew ? `Create ${ct.display_name}` : `Edit ${ct.display_name}`}</h1>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--primary" onClick={save} disabled={saving}>
            {saving ? "Saving…" : isNew ? "Create" : "Save"}
          </button>
        </div>
      </div>

      {banner && <div className="rs-login-error" style={{ margin: "0 24px" }}>{banner}</div>}

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-fields">
            {ct.fields.map((f) => (
              <FieldRow
                key={f.name}
                field={f}
                value={form[f.name]}
                error={fieldErrors[f.name]}
                onChange={(v) => set(f.name, v)}
                type={type}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function FieldRow({
  field,
  value,
  error,
  onChange,
  type,
}: {
  field: Field;
  value: unknown;
  error?: string;
  onChange: (v: unknown) => void;
  type: string;
}) {
  return (
    <div className="rs-field">
      <div className="rs-field-label">
        <label>
          {field.name}
          {field.required && <span className="rs-req">*</span>}
        </label>
        <span className="rs-field-hint">{field.kind}</span>
      </div>
      <FieldInput field={field} value={value} onChange={onChange} type={type} />
      {error && <div className="rs-login-error">{error}</div>}
    </div>
  );
}

function FieldInput({
  field,
  value,
  onChange,
  type: _type,
}: {
  field: Field;
  value: unknown;
  onChange: (v: unknown) => void;
  type: string;
}) {
  const str = typeof value === "string" ? value : value == null ? "" : String(value);
  switch (field.kind) {
    case "text":
    case "json":
      return (
        <textarea
          className="rs-input rs-textarea"
          rows={field.kind === "json" ? 6 : 3}
          value={typeof value === "object" && value !== null ? JSON.stringify(value, null, 2) : str}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "integer":
    case "float":
      return (
        <input
          className="rs-input"
          type="number"
          value={str}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "boolean":
      return (
        <button
          className={"rs-toggle" + (value ? " is-on" : "")}
          onClick={() => onChange(!value)}
          type="button"
        >
          <span className="rs-toggle-knob" />
        </button>
      );
    case "datetime":
      return (
        <input
          className="rs-input"
          type="datetime-local"
          value={str ? str.slice(0, 16) : ""}
          onChange={(e) => onChange(e.target.value ? new Date(e.target.value).toISOString() : "")}
        />
      );
    case "enum":
      return (
        <select className="rs-input" value={str} onChange={(e) => onChange(e.target.value)}>
          <option value="">—</option>
          {enumValues(field).map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      );
    case "relation":
      return <RelationSelect field={field} value={str} onChange={onChange} />;
    default:
      return (
        <input className="rs-input" value={str} onChange={(e) => onChange(e.target.value)} />
      );
  }
}

function RelationSelect({
  field,
  value,
  onChange,
}: {
  field: Field;
  value: string;
  onChange: (v: unknown) => void;
}) {
  const meta = relationMeta(field);
  const target = meta?.target ?? "";
  const opts = useResource(
    () => (target ? listEntries(target, { pageSize: 100 }) : Promise.resolve(null)),
    [target],
  );
  const targetSchema = useResource(
    () => (target ? getContentType(target) : Promise.resolve(null)),
    [target],
  );
  return (
    <select className="rs-input" value={value} onChange={(e) => onChange(e.target.value)}>
      <option value="">—</option>
      {opts.data?.data.map((e: Entry) => (
        <option key={e.id} value={e.id}>
          {relationLabel(e, targetSchema.data ?? undefined)}
        </option>
      ))}
    </select>
  );
}
