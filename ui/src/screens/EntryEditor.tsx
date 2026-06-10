import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Notice, LoadingState, EmptyState, EditorBar } from "../components/ui";
import { StatusBadge } from "../components/shell";
import { useResource } from "../hooks/useResource";
import { FieldRow } from "../components/FieldInput";
import {
  createEntry,
  getContentType,
  getEntry,
  publishEntry,
  unpublishEntry,
  updateEntry,
} from "../api/endpoints";
import { draftPublishEnabled } from "../api/types";
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
        navigate(`/content/${type}`, { state: { flash: "created", flashId: created.id } });
      } else {
        await updateEntry(type, id, body);
        navigate(`/content/${type}`, { state: { flash: "saved", flashId: id } });
      }
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
