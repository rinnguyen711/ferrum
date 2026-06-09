import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { Notice, LoadingState, EmptyState, EditorBar } from "../components/ui";
import { StatusBadge } from "../components/shell";
import { useResource } from "../hooks/useResource";
import { FieldRow } from "../components/FieldInput";
import {
  getContentType,
  getSingleType,
  putSingleType,
  publishEntry,
  unpublishEntry,
} from "../api/endpoints";
import { draftPublishEnabled } from "../api/types";
import { ApiError } from "../api/client";

export function SingleTypeEdit() {
  const { type = "" } = useParams<{ type: string }>();

  const schema = useResource(() => getContentType(type), [type]);
  const existing = useResource(() => getSingleType(type), [type]);

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
    if (schema.data && !existing.loading) {
      const seed: Record<string, unknown> = {};
      for (const f of schema.data.fields) {
        seed[f.name] = existing.data ? existing.data[f.name] ?? "" : "";
      }
      setForm(seed);
    }
  }, [schema.data, existing.data, existing.loading]);

  if (schema.loading || existing.loading) return <LoadingState />;
  if (schema.error) return <EmptyState>Couldn't load type. {schema.error.message}</EmptyState>;
  if (existing.error) return <EmptyState>{existing.error.message}</EmptyState>;
  const ct = schema.data;
  if (!ct) return <EmptyState>Unknown content type.</EmptyState>;

  const dp = draftPublishEnabled(ct);
  const isPublished = publishedAt != null;
  const entryId = existing.data?.id as string | undefined;

  const set = (name: string, value: unknown) =>
    setForm((f) => ({ ...f, [name]: value }));

  const togglePublish = async () => {
    if (!entryId) return;
    setPublishing(true);
    try {
      const updated = isPublished
        ? await unpublishEntry(ct.name, entryId)
        : await publishEntry(ct.name, entryId);
      setPublishedAt((updated.published_at as string | null) ?? null);
    } catch {
      setBanner("Publish action failed.");
    } finally {
      setPublishing(false);
    }
  };

  const save = async () => {
    setSaving(true);
    setFieldErrors({});
    setBanner(null);
    // Build body: omit empty strings; coerce numbers.
    const body: Record<string, unknown> = {};
    for (const f of ct.fields) {
      const v = form[f.name];
      if (f.kind === "media") {
        if (Array.isArray(v)) { body[f.name] = v; }
        else if (v == null || v === "") { /* single unset: omit */ }
        else { body[f.name] = v; }
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
      const saved = await putSingleType(type, body);
      setPublishedAt((saved.published_at as string | null) ?? null);
      setBanner("Saved.");
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
        title={ct.display_name}
        status={dp ? <StatusBadge status={isPublished ? "published" : "draft"} /> : undefined}
        actions={
          <>
            {dp && entryId && (
              <button
                className={"rs-btn " + (isPublished ? "rs-btn--ghost" : "rs-btn--primary")}
                onClick={togglePublish}
                disabled={publishing}
              >
                {publishing ? "…" : isPublished ? "Unpublish" : "Publish"}
              </button>
            )}
            <button
              className={"rs-btn " + (dp ? "rs-btn--ghost" : "rs-btn--primary")}
              onClick={save}
              disabled={saving}
            >
              {saving ? "Saving…" : "Save"}
            </button>
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
