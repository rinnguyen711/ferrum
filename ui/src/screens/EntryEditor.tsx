import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Notice, LoadingState, EmptyState, EditorBar } from "../components/ui";
import { StatusBadge } from "../components/shell";
import { useResource } from "../hooks/useResource";
import { RichTextEditor } from "../components/RichTextEditor";
import {
  createEntry,
  getContentType,
  getEntry,
  listAssets,
  listEntries,
  publishEntry,
  unpublishEntry,
  updateEntry,
} from "../api/endpoints";
import { Plus, Link2, Trash2, Check, GripVertical } from "lucide-react";
import { AssetPicker } from "./media/AssetPicker";
import { AssetThumb } from "./media/AssetThumb";
import type { Entry, Field, MediaAsset } from "../api/types";
import { draftPublishEnabled, enumValues, mediaMeta, relationMeta } from "../api/types";
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
  const [publishedAt, setPublishedAt] = useState<string | null>(null);
  const [publishing, setPublishing] = useState(false);

  useEffect(() => {
    setPublishedAt((existing.data?.published_at as string | null) ?? null);
  }, [existing.data]);

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

  if (schema.loading || existing.loading) return <LoadingState />;
  if (schema.error) return <EmptyState>Couldn't load type. {schema.error.message}</EmptyState>;
  if (existing.error) return <EmptyState>{existing.error.message}</EmptyState>;
  const ct = schema.data;
  if (!ct) return <EmptyState>Unknown content type.</EmptyState>;

  const dp = ct ? draftPublishEnabled(ct) : false;
  const isPublished = publishedAt != null;

  const set = (name: string, value: unknown) =>
    setForm((f) => ({ ...f, [name]: value }));

  const togglePublish = async () => {
    if (!ct) return;
    setPublishing(true);
    try {
      const updated = isPublished
        ? await unpublishEntry(ct.name, id)
        : await publishEntry(ct.name, id);
      setPublishedAt((updated.published_at as string | null) ?? null);
    } catch {
      setBanner("Publish action failed.");
    } finally {
      setPublishing(false);
    }
  };

  const save = async (publishAfter = false) => {
    setSaving(true);
    setFieldErrors({});
    setBanner(null);
    // Build a body: omit empty strings (treated as "no value"); coerce numbers.
    const body: Record<string, unknown> = {};
    for (const f of ct.fields) {
      const v = form[f.name];
      if (f.kind === "media") {
        if (Array.isArray(v)) { body[f.name] = v; }            // multiple: always send (even [])
        else if (v == null || v === "") { /* single unset: omit */ }
        else { body[f.name] = v; }                              // single: id string
        continue;
      }
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
      if (isNew) {
        const created = await createEntry(type, body);
        if (publishAfter) await publishEntry(type, created.id);
      } else {
        await updateEntry(type, id, body);
      }
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

      {banner && <div style={{ margin: "0 24px" }}><Notice>{banner}</Notice></div>}

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

const WIDE_KINDS = new Set(["text", "json", "rich_text", "media"]);

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
    case "media":
      return <MediaField field={field} value={value} onChange={onChange} />;
    case "rich_text":
      return <RichTextEditor value={value} onChange={onChange} />;
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
