import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { useResource } from "../hooks/useResource";
import {
  deleteContentType, getContentType, listContentTypes,
} from "../api/endpoints";
import { ApiError } from "../api/client";
import { enumValues, relationMeta } from "../api/types";
import { useBuilderDraft } from "./BuilderDraftContext";
import { blankField, type DraftField } from "./draftModel";
import { FieldRow } from "./FieldRow";

export function SchemaEditor() {
  const { type } = useParams<{ type: string }>();
  const navigate = useNavigate();
  const {
    draft, banner, fieldErrors, loadExisting, setDraft, clearBanner,
  } = useBuilderDraft();
  const allTypes = useResource(() => listContentTypes(), []);

  // Existing-type route: load from server (once per :type).
  useEffect(() => {
    if (!type) return;
    if (draft && draft.mode === "existing" && draft.name === type) return; // already seeded (e.g. just created)
    let ignore = false;
    getContentType(type)
      .then((ct) => { if (!ignore) loadExisting(ct); })
      .catch(() => { /* banner handled below via missing draft */ });
    return () => { ignore = true; };
  }, [type, loadExisting, draft]);

  // New-type route with no draft (direct hit / reload) → back to picker.
  useEffect(() => {
    if (!type && !draft) navigate("/builder", { replace: true });
  }, [type, draft, navigate]);

  // Delete (existing only).
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [delBanner, setDelBanner] = useState<string | null>(null);
  const doDelete = async () => {
    if (!type) return;
    setDeleting(true);
    setDelBanner(null);
    try {
      await deleteContentType(type);
      navigate("/builder");
    } catch (e) {
      setDelBanner(e instanceof ApiError ? e.message : "Delete failed.");
      setConfirming(false);
    } finally {
      setDeleting(false);
    }
  };

  // Staged drops: existing fields the user removed (kept visible, greyed).
  const [stagedDrops, setStagedDrops] = useState<Set<string>>(new Set());

  // Reset per-type local UI state when switching between types.
  useEffect(() => {
    setStagedDrops(new Set());
    setConfirming(false);
    setDeleting(false);
    setDelBanner(null);
  }, [type]);

  if (!draft) return <div className="rs-empty">Loading…</div>;

  const snapshot = draft.serverSnapshot;

  // Warn: a new field reusing the name of a dropped existing field = rename (unsupported).
  const renameCollisions = (snapshot?.fields ?? [])
    .filter((sf) =>
      !draft.fields.some((d) => d.name === sf.name && d.origin === "existing") &&
      draft.fields.some((d) => d.name === sf.name && d.origin === "new"),
    )
    .map((sf) => sf.name);
  const lockedEnum = (d: DraftField): string[] => {
    const orig = snapshot?.fields.find((f) => f.name === d.name);
    return orig ? enumValues(orig) : [];
  };

  const updateField = (id: string, patch: Partial<DraftField>) =>
    setDraft((d) => ({ ...d, fields: d.fields.map((f) => (f.id === id ? { ...f, ...patch } : f)) }));

  const removeField = (f: DraftField) => {
    if (f.origin === "existing") {
      setStagedDrops((s) => {
        const next = new Set(s);
        next.add(f.name);
        return next;
      });
      setDraft((d) => ({ ...d, fields: d.fields.filter((x) => x.id !== f.id) }));
    } else {
      setDraft((d) => ({ ...d, fields: d.fields.filter((x) => x.id !== f.id) }));
    }
  };

  const addField = () =>
    setDraft((d) => ({ ...d, fields: [...d.fields, blankField()] }));

  const unstage = (name: string) => {
    setStagedDrops((s) => { const n = new Set(s); n.delete(name); return n; });
    const orig = snapshot?.fields.find((f) => f.name === name);
    if (!orig) return;
    const rel = relationMeta(orig);
    setDraft((d) => ({
      ...d,
      fields: [...d.fields, {
        id: crypto.randomUUID(),
        name: orig.name, kind: orig.kind, required: orig.required,
        unique: orig.unique, enumValues: enumValues(orig),
        target: rel?.target ?? "", inverse: rel?.inverse ?? "", origin: "existing" as const,
      }],
    }));
  };

  // Rows for staged (removed) existing fields, shown greyed with an un-stage button.
  const stagedRows: DraftField[] = (snapshot?.fields ?? [])
    .filter((f) => stagedDrops.has(f.name) && !draft.fields.some((d) => d.name === f.name))
    .map((f) => ({
      id: "staged-" + f.name,
      name: f.name,
      kind: f.kind,
      required: f.required,
      unique: f.unique,
      enumValues: enumValues(f),
      target: "",
      inverse: "",
      origin: "existing" as const,
    }));

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <input
            className="rs-input rs-title-input"
            value={draft.display_name}
            onChange={(e) => { clearBanner(); setDraft((d) => ({ ...d, display_name: e.target.value })); }}
            placeholder="Display name"
          />
          <p className="rs-cm-sub rs-mono">
            {draft.name} · {draft.fields.length} fields
            {draft.mode === "new" ? " · unsaved" : ""}
          </p>
        </div>
        {draft.mode === "existing" && (
          confirming ? (
            <div className="rs-confirm">
              <span>Delete <strong>{draft.name}</strong>? Drops the type and all its entries.</span>
              <button className="rs-btn rs-btn--ghost rs-btn--sm rs-danger" onClick={doDelete} disabled={deleting}>
                {deleting ? "Deleting…" : "Confirm"}
              </button>
              <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setConfirming(false)} disabled={deleting}>
                Cancel
              </button>
            </div>
          ) : (
            <button className="rs-btn rs-btn--ghost rs-danger" onClick={() => setConfirming(true)}>
              Delete type
            </button>
          )
        )}
      </div>

      {banner && <div className="rs-login-error" style={{ marginBottom: 12 }}>{banner}</div>}
      {delBanner && <div className="rs-login-error" style={{ marginBottom: 12 }}>{delBanner}</div>}
      {renameCollisions.length > 0 && (
        <div className="rs-login-error" style={{ marginBottom: 12 }}>
          Field{renameCollisions.length > 1 ? "s" : ""} {renameCollisions.join(", ")}{" "}
          already exist on this type. Renaming or changing a field's type is not
          supported — remove the new field or pick a different name.
        </div>
      )}

      <h2 className="rs-cm-sub" style={{ marginTop: 20 }}>Fields</h2>
      <div className="rs-fieldrows">
        {draft.fields.map((f) => (
          <FieldRow
            key={f.id}
            field={f}
            error={fieldErrors[f.name]}
            typeNames={allTypes.data?.map((t) => t.name) ?? []}
            lockedEnumValues={lockedEnum(f)}
            staged={false}
            onChange={(patch) => updateField(f.id, patch)}
            onRemove={() => removeField(f)}
          />
        ))}
        {stagedRows.map((f) => (
          <FieldRow
            key={f.id}
            field={f}
            typeNames={allTypes.data?.map((t) => t.name) ?? []}
            lockedEnumValues={f.enumValues}
            staged={true}
            onChange={() => {}}
            onRemove={() => unstage(f.name)}
          />
        ))}
      </div>
      <button className="rs-btn rs-btn--ghost" onClick={addField} style={{ marginTop: 12 }}>
        <Icons.plus size={15} /> Add field
      </button>
    </div>
  );
}
