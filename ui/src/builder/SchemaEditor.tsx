import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { useResource } from "../hooks/useResource";
import {
  deleteContentType, getContentType, listContentTypes,
} from "../api/endpoints";
import { ApiError } from "../api/client";
import { enumValues } from "../api/types";
import { useBuilderDraft } from "./BuilderDraftContext";
import { blankField, type DraftField } from "./draftModel";
import { FieldRow } from "./FieldRow";
import { FieldConfigModal } from "./FieldConfigModal";

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

  // Field edit modal: { field, isNew } when open, null when closed.
  const [modal, setModal] = useState<{ field: DraftField; isNew: boolean } | null>(null);

  // Reset per-type local UI state when switching between types.
  useEffect(() => {
    setModal(null);
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

  const removeField = (f: DraftField) =>
    setDraft((d) => ({ ...d, fields: d.fields.filter((x) => x.id !== f.id) }));

  const addField = () => setModal({ field: blankField(), isNew: true });

  const editField = (f: DraftField) => setModal({ field: f, isNew: false });

  const saveField = (f: DraftField) => {
    setDraft((d) => {
      const exists = d.fields.some((x) => x.id === f.id);
      return exists
        ? { ...d, fields: d.fields.map((x) => (x.id === f.id ? f : x)) }
        : { ...d, fields: [...d.fields, f] };
    });
    setModal(null);
  };

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
            api::{draft.name}.{draft.name} · {draft.fields.length} fields · collection type
            {draft.mode === "new" ? " · unsaved" : ""}
          </p>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon">
            <Icons.eye size={15} /> Preview API
          </button>
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
      {Object.keys(fieldErrors).length > 0 && (
        <div className="rs-login-error" style={{ marginBottom: 12 }}>
          {Object.entries(fieldErrors).map(([name, msg]) => (
            <div key={name}><strong className="rs-mono">{name}</strong>: {msg}</div>
          ))}
        </div>
      )}

      <div className="rs-schema">
        <div className="rs-schema-head"><span>Field</span><span>Type</span><span></span></div>
        {draft.fields.map((f) => (
          <FieldRow
            key={f.id}
            field={f}
            onEdit={() => editField(f)}
            onRemove={() => removeField(f)}
          />
        ))}
        <button className="rs-schema-add" onClick={addField}>
          <Icons.plus size={16} /> Add another field to this collection type
        </button>
      </div>

      {modal && (
        <FieldConfigModal
          initial={modal.field}
          isNew={modal.isNew}
          typeNames={allTypes.data?.map((t) => t.name) ?? []}
          lockedEnumValues={lockedEnum(modal.field)}
          onSave={saveField}
          onClose={() => setModal(null)}
        />
      )}
    </div>
  );
}
