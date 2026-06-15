import { useEffect, useRef, useState } from "react";
import { Plus, Link2, Trash2, Check, GripVertical } from "lucide-react";
import { RichTextEditor } from "./RichTextEditor";
import { AssetPicker } from "../screens/media/AssetPicker";
import { AssetThumb } from "../screens/media/AssetThumb";
import { Notice } from "./ui";
import {
  getContentType,
  listAssets,
  listEntries,
} from "../api/endpoints";
import type { Entry, Field, MediaAsset } from "../api/types";
import {
  componentMeta,
  enumValues,
  mediaMeta,
  relationMeta,
} from "../api/types";
import { relationLabel } from "../util";
import { useResource } from "../hooks/useResource";

// ── FieldRow ──────────────────────────────────────────────────────────────────

const WIDE_KINDS = new Set(["text", "json", "rich_text", "media", "component"]);

export function FieldRow({
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
  const wide = WIDE_KINDS.has(field.kind);
  return (
    <div className="rs-field" data-wide={wide ? "true" : undefined}>
      <div className="rs-field-label">
        <label>
          {field.name}
          {field.required && <span className="rs-req">*</span>}
        </label>
        <span className="rs-field-hint">{field.kind}</span>
      </div>
      <FieldInput field={field} value={value} onChange={onChange} type={type} />
      {error && <Notice>{error}</Notice>}
    </div>
  );
}

// ── FieldInput ────────────────────────────────────────────────────────────────

export function FieldInput({
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
    case "media":
      return <MediaField field={field} value={value} onChange={onChange} />;
    case "rich_text":
      return <RichTextEditor value={value} onChange={onChange} />;
    case "component":
      return <ComponentField field={field} value={value} onChange={onChange} />;
    default:
      return (
        <input className="rs-input" value={str} onChange={(e) => onChange(e.target.value)} />
      );
  }
}

// ── RelationSelect ────────────────────────────────────────────────────────────

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

// ── MediaField ────────────────────────────────────────────────────────────────

function MediaField({
  field,
  value,
  onChange,
}: {
  field: Field;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const multiple = mediaMeta(field)?.multiple ?? false;
  const [open, setOpen] = useState(false);
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);
  const dragSrc = useRef<number | null>(null);
  const selfChange = useRef(false);

  useEffect(() => {
    if (selfChange.current) { selfChange.current = false; return; }
    let cancelled = false;
    const seed = async () => {
      if (value === "" || value == null) { setAssets([]); return; }
      const items = Array.isArray(value) ? value : [value];
      const objects = items.filter(
        (x): x is MediaAsset => typeof x === "object" && x !== null && "id" in (x as object),
      );
      if (objects.length === items.length) { setAssets(objects); return; }
      const ids = items
        .map((x) => (typeof x === "string" ? x : (x as MediaAsset)?.id))
        .filter(Boolean) as string[];
      try {
        const all = await listAssets(null);
        if (cancelled) return;
        const byId = new Map(all.map((a) => [a.id, a]));
        setAssets(ids.map((id) => byId.get(id)).filter((a): a is MediaAsset => !!a));
      } catch {
        if (!cancelled) setAssets([]);
      }
    };
    seed();
    return () => { cancelled = true; };
  }, [value]);

  const emit = (next: MediaAsset[]) => {
    selfChange.current = true;
    setAssets(next);
    onChange(multiple ? next.map((a) => a.id) : (next[0]?.id ?? null));
  };

  const onPick = (picked: MediaAsset[]) => {
    setOpen(false);
    if (multiple) {
      const existing = new Set(assets.map((a) => a.id));
      emit([...assets, ...picked.filter((p) => !existing.has(p.id))]);
    } else {
      emit(picked.slice(0, 1));
    }
  };

  const remove = (id: string) => emit(assets.filter((a) => a.id !== id));

  const copyLink = (id: string) => {
    const url = `${window.location.origin}/admin/media/assets/${id}/raw`;
    navigator.clipboard.writeText(url).then(() => {
      setCopiedId(id);
      setTimeout(() => setCopiedId((c) => (c === id ? null : c)), 1800);
    }).catch(() => {});
  };

  const onDragStart = (i: number) => { dragSrc.current = i; };
  const onDragEnter = (i: number) => { if (dragSrc.current !== null && dragSrc.current !== i) setDragOver(i); };
  const onDragEnd = () => { setDragOver(null); dragSrc.current = null; };
  const onDrop = (i: number) => {
    const src = dragSrc.current;
    if (src === null || src === i) return;
    const next = assets.slice();
    const [item] = next.splice(src, 1);
    next.splice(i, 0, item);
    emit(next);
    setDragOver(null);
    dragSrc.current = null;
  };

  const showAdd = multiple || assets.length === 0;

  return (
    <div className="rs-media-field">
      {assets.length > 0 && (
        <div className="rs-media-field-grid">
          {assets.map((a, i) => (
            <div
              key={a.id}
              className={"rs-media-field-card" + (dragOver === i ? " is-drag-over" : "")}
              draggable={multiple}
              onDragStart={multiple ? () => onDragStart(i) : undefined}
              onDragEnter={multiple ? (e) => { e.preventDefault(); onDragEnter(i); } : undefined}
              onDragOver={multiple ? (e) => e.preventDefault() : undefined}
              onDragLeave={multiple ? () => setDragOver(null) : undefined}
              onDragEnd={multiple ? onDragEnd : undefined}
              onDrop={multiple ? (e) => { e.preventDefault(); onDrop(i); } : undefined}
            >
              {multiple && <div className="rs-media-field-grip"><GripVertical size={13} /></div>}
              <div className="rs-media-field-thumb-wrap">
                <AssetThumb asset={a} className="rs-media-field-thumb" />
                <div className="rs-media-field-overlay">
                  <div className="rs-media-field-act-wrap" style={{ position: "relative" }}>
                    <button
                      type="button"
                      className={"rs-media-field-act" + (copiedId === a.id ? " is-copied" : "")}
                      title="Copy link"
                      onClick={() => copyLink(a.id)}
                    >
                      {copiedId === a.id ? <Check size={15} /> : <Link2 size={15} />}
                    </button>
                    {copiedId === a.id && <span className="rs-media-field-copied">Copied!</span>}
                  </div>
                  <button type="button" className="rs-media-field-act rs-media-field-act--danger" title="Remove" onClick={() => remove(a.id)}>
                    <Trash2 size={15} />
                  </button>
                </div>
              </div>
              <p className="rs-media-field-name" title={a.file_name}>{a.original_filename}</p>
            </div>
          ))}
        </div>
      )}
      {showAdd && (
        <button type="button" className="rs-media-field-add" onClick={() => setOpen(true)}>
          <Plus size={15} />
          <span>{assets.length === 0 ? "Add asset" : "Add more"}</span>
        </button>
      )}
      {open && <AssetPicker multiple={multiple} onClose={() => setOpen(false)} onPick={onPick} />}
    </div>
  );
}

// ── ComponentField ────────────────────────────────────────────────────────────

function ComponentField({
  field,
  value,
  onChange,
}: {
  field: Field;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const meta = componentMeta(field);
  const innerFields = (field.kind_meta._component_fields as Field[] | undefined) ?? field._component_fields ?? [];

  if (!meta) return null;

  if (meta.multiple) {
    const arr = Array.isArray(value) ? (value as Record<string, unknown>[]) : [];
    const setItem = (i: number, patch: Record<string, unknown>) => {
      const next = arr.slice();
      next[i] = { ...next[i], ...patch };
      onChange(next);
    };
    const addItem = () => onChange([...arr, {}]);
    const removeItem = (i: number) => onChange(arr.filter((_, idx) => idx !== i));
    return (
      <div className="rs-component-list">
        {arr.map((item, i) => (
          <div key={i} className="rs-component-card">
            <div className="rs-component-card-head">
              <span style={{ fontWeight: 500, fontSize: 12, color: "var(--rs-fg-muted)" }}>#{i + 1}</span>
              <button type="button" className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => removeItem(i)}>
                <Trash2 size={13} />
              </button>
            </div>
            {innerFields.map((f) => (
              <FieldRow
                key={f.name}
                field={f}
                value={item[f.name]}
                onChange={(v) => setItem(i, { [f.name]: v })}
                type=""
              />
            ))}
          </div>
        ))}
        <button type="button" className="rs-btn rs-btn--ghost rs-btn--sm" onClick={addItem}>
          <Plus size={13} /> Add item
        </button>
      </div>
    );
  }

  const obj = (value && typeof value === "object" && !Array.isArray(value))
    ? (value as Record<string, unknown>)
    : {};
  const setField = (name: string, v: unknown) => onChange({ ...obj, [name]: v });

  return (
    <div className="rs-component-card">
      {innerFields.map((f) => (
        <FieldRow
          key={f.name}
          field={f}
          value={obj[f.name]}
          onChange={(v) => setField(f.name, v)}
          type=""
        />
      ))}
    </div>
  );
}
