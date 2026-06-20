import { useEffect, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
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
import { draftPublishEnabled, coerceFieldValue, localizedEnabled } from "../api/types";
import { listLocales, type Locale } from "../api/locales";
import { ApiError } from "../api/client";

export function EntryEditor() {
  const { type = "", id = "new" } = useParams<{ type: string; id: string }>();
  const navigate = useNavigate();
  const isNew = id === "new";
  const onBack = () => navigate(`/content/${type}`);

  const schema = useResource(() => getContentType(type), [type]);
  const [searchParams, setSearchParams] = useSearchParams();
  const loc = schema.data ? localizedEnabled(schema.data) : false;
  const localesRes = useResource(() => (loc ? listLocales() : Promise.resolve([] as Locale[])), [loc]);
  const requestedLocale = searchParams.get("locale") ?? "";
  const existing = useResource(
    () =>
      isNew
        ? Promise.resolve(null)
        : getEntry(type, id, loc ? { locale: requestedLocale || undefined } : {}),
    [type, id, isNew, loc, requestedLocale],
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

  // For localized types: the locale actually served (may differ from requested
  // when the backend fell back to the default-locale row).
  const servedLocale = (existing.data?.locale as string | undefined) ?? requestedLocale;
  // A translation for the requested locale is "missing" when the server served
  // a different locale (fallback) — the user is about to create a new one.
  const missingTranslation =
    loc && !isNew && requestedLocale !== "" && servedLocale !== requestedLocale;

  // When the requested locale has no translation yet, blank the form so the
  // translator starts from an empty row instead of the fallback's values.
  useEffect(() => {
    if (missingTranslation && schema.data) {
      const blank: Record<string, unknown> = {};
      for (const f of schema.data.fields) blank[f.name] = "";
      setForm(blank);
    }
  }, [missingTranslation, schema.data]);

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

  // Build the request body from the form, applying per-kind coercion (media
  // single/multiple, integer/float Number(), json parse, component coerce).
  // Returns null if a JSON field fails to parse, after setting the field error.
  const buildBody = (): Record<string, unknown> | null => {
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
          return null;
        }
      } else if (f.kind === "component") {
        // coerce nested number sub-fields (inputs emit strings) before sending
        body[f.name] = coerceFieldValue(f, v);
      } else {
        body[f.name] = v;
      }
    }
    return body;
  };

  const createTranslation = async () => {
    if (!ct) return;
    setSaving(true);
    setFieldErrors({});
    setBanner(null);
    const body = buildBody();
    if (!body) {
      setSaving(false);
      return;
    }
    body.document_id = id;
    try {
      await createEntry(type, body, { locale: requestedLocale });
      navigate(`/content/${type}?locale=${encodeURIComponent(requestedLocale)}`, {
        state: { flash: "created", flashId: id },
      });
    } catch (e) {
      setBanner(e instanceof ApiError ? e.message : "Couldn't create translation.");
    } finally {
      setSaving(false);
    }
  };

  const save = async (publishAfter = false) => {
    setSaving(true);
    setFieldErrors({});
    setBanner(null);
    const body = buildBody();
    if (!body) {
      setSaving(false);
      return;
    }
    try {
      if (isNew) {
        const created = await createEntry(type, body, loc ? { locale: requestedLocale || undefined } : {});
        if (publishAfter) await publishEntry(type, created.id);
        navigate(`/content/${type}`, { state: { flash: "created", flashId: created.id } });
      } else {
        await updateEntry(type, id, body, loc ? { locale: requestedLocale || undefined } : {});
        navigate(`/content/${type}`, { state: { flash: "saved", flashId: id } });
      }
    } catch (e) {
      if (e instanceof ApiError) {
        if (e.fieldErrors.length) {
          const map: Record<string, string> = {};
          for (const fe of e.fieldErrors) map[fe.field] = fe.message ?? "Invalid";
          setFieldErrors(map);
          // Safety net: surface a banner too so the failure is never silent,
          // even if a field key (e.g. a nested component path) doesn't map to
          // a rendered input.
          setBanner(
            e.fieldErrors
              .map((fe) => `${fe.field}: ${fe.message ?? "invalid"}`)
              .join("; "),
          );
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

  const switchLocale = (code: string) =>
    setSearchParams((p) => {
      p.set("locale", code);
      return p;
    });

  const localeSwitcher =
    loc && localesRes.data && localesRes.data.length > 0 ? (
      <select
        className="rs-input rs-input--sm"
        value={requestedLocale || servedLocale}
        onChange={(e) => switchLocale(e.target.value)}
        aria-label="Locale"
      >
        {localesRes.data.map((l) => (
          <option key={l.code} value={l.code}>
            {l.code} — {l.name}
          </option>
        ))}
      </select>
    ) : null;

  return (
    <div className="rs-editor">
      <EditorBar
        onBack={onBack}
        title={isNew ? `Create ${ct.display_name}` : `Edit ${ct.display_name}`}
        status={
          <>
            {dp && !isNew && <StatusBadge status={isPublished ? "published" : "draft"} />}
            {localeSwitcher}
          </>
        }
        actions={
          missingTranslation ? (
            <button className="rs-btn rs-btn--primary" onClick={createTranslation} disabled={saving}>
              {saving ? "Creating…" : `Create ${requestedLocale} translation`}
            </button>
          ) : (
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
          )
        }
      />

      {banner && <div style={{ margin: "0 24px" }}><Notice>{banner}</Notice></div>}

      {missingTranslation && (
        <div style={{ margin: "0 24px" }}>
          <Notice>No translation for “{requestedLocale}” yet. Fill the fields and create one.</Notice>
        </div>
      )}

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-fields">
            {ct.fields.map((f) => (
              <FieldRow
                key={f.name}
                field={f}
                value={form[f.name]}
                error={fieldErrors[f.name]}
                errors={fieldErrors}
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
