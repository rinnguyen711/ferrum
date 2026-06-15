import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Notice, LoadingState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import {
  deleteContentType, getContentType, listContentTypes, listComponents,
} from "../api/endpoints";
import { ApiError } from "../api/client";
import { draftPublishEnabled, enumValues, managedType } from "../api/types";
import type { FieldKind } from "../api/types";
import { useBuilderDraft } from "./BuilderDraftContext";
import { blankField, type Draft, type DraftField } from "./draftModel";
import { FieldRow } from "./FieldRow";
import { FieldConfigModal } from "./FieldConfigModal";
import { FieldPicker } from "./FieldPicker";
import { SaveBar } from "./SaveBar";

function DeleteTypeModal({
  typeName,
  deleting,
  error,
  onConfirm,
  onClose,
}: {
  typeName: string;
  deleting: boolean;
  error: string | null;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === "Escape" && !deleting) onCloseRef.current(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [deleting]);

  return (
    <div className="rs-modal-backdrop" onClick={() => { if (!deleting) onClose(); }}>
      <div
        className="rs-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 420 }}
      >
        <div className="rs-modal-head">
          <div className="rs-modal-ico" style={{ background: "var(--danger-soft, var(--surface-3))", color: "var(--danger)" }}>
            <Icons.trash size={18} />
          </div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">Destructive action</span>
            <h2>Delete "{typeName}"?</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose} disabled={deleting} aria-label="Close">
            <Icons.x size={18} />
          </button>
        </div>
        <div className="rs-modal-body">
          <p style={{ fontSize: 14, color: "var(--text-muted)", margin: 0 }}>
            This will permanently drop the <strong className="rs-mono">{typeName}</strong> content type and <strong>all its entries</strong>. This cannot be undone.
          </p>
          {error && <div style={{ marginTop: 12 }}><Notice>{error}</Notice></div>}
        </div>
        <div className="rs-modal-foot" style={{ justifyContent: "space-between" }}>
          <button className="rs-btn rs-btn--ghost" onClick={onClose} disabled={deleting}>
            Cancel
          </button>
          <button
            className="rs-btn rs-btn--primary"
            onClick={onConfirm}
            disabled={deleting}
            style={{ background: "var(--danger)", borderColor: "var(--danger)", color: "#fff" }}
          >
            {deleting ? "Deleting…" : "Delete type"}
          </button>
        </div>
      </div>
    </div>
  );
}

export function SchemaEditor() {
  const { type } = useParams<{ type: string }>();
  const navigate = useNavigate();
  const {
    draft, banner, fieldErrors, loadExisting, setDraft, clearBanner,
  } = useBuilderDraft();
  const allTypes = useResource(() => listContentTypes(), []);
  const allComponents = useResource(() => listComponents(), []);

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

  // Field modal: "pick" = choosing a type, "config" = editing one, null = closed.
  type FieldModal =
    | { step: "pick" }
    | { step: "config"; field: DraftField; isNew: boolean };
  const [modal, setModal] = useState<FieldModal | null>(null);

  // Reset per-type local UI state when switching between types.
  useEffect(() => {
    setModal(null);
    setConfirming(false);
    setDeleting(false);
    setDelBanner(null);
  }, [type]);

  if (!draft || draft.mode === "component") return <LoadingState />;

  // Narrowed setter — this editor only ever touches content-type drafts.
  const setTypeDraft = (fn: (d: Draft) => Draft) =>
    setDraft((d) => (d.mode === "component" ? d : fn(d)));

  const snapshot = draft.serverSnapshot;
  const isManaged = snapshot ? managedType(snapshot) : false;

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
    setTypeDraft((d) => ({ ...d, fields: d.fields.filter((x) => x.id !== f.id) }));

  const addField = () => setModal({ step: "pick" });

  const pickKind = (kind: FieldKind) =>
    setModal({ step: "config", field: blankField(kind), isNew: true });

  const editField = (f: DraftField) =>
    setModal({ step: "config", field: f, isNew: false });

  const saveField = (f: DraftField) => {
    setTypeDraft((d) => {
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
            onChange={(e) => { clearBanner(); setTypeDraft((d) => ({ ...d, display_name: e.target.value })); }}
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
          <button
            className="rs-btn rs-btn--ghost rs-danger"
            onClick={() => setConfirming(true)}
            disabled={isManaged}
          >
            Delete type
          </button>
        )}
        </div>
      </div>

      {isManaged && (
        <Notice tone="ok">
          Managed by a schema file — edit the TOML and restart to change this type.
        </Notice>
      )}
      {banner && <Notice>{banner}</Notice>}
      {renameCollisions.length > 0 && (
        <Notice>
          Field{renameCollisions.length > 1 ? "s" : ""} {renameCollisions.join(", ")}{" "}
          already exist on this type. Renaming or changing a field's type is not
          supported — remove the new field or pick a different name.
        </Notice>
      )}
      {Object.keys(fieldErrors).length > 0 && (
        <Notice>
          {Object.entries(fieldErrors).map(([name, msg]) => (
            <div key={name}><strong className="rs-mono">{name}</strong>: {msg}</div>
          ))}
        </Notice>
      )}

      <SaveBar disabled={isManaged} />

      <div className="rs-setting-row" style={{ marginBottom: 16 }}>
        <div className="rs-setting-meta">
          <strong>Enable Draft &amp; Publish</strong>
          <span>Entries go through a draft state before being published.</span>
        </div>
        <button
          type="button"
          className={"rs-toggle" + (draft.draft_publish ? " is-on" : "")}
          disabled={draft.mode === "existing" && (draft.serverSnapshot ? draftPublishEnabled(draft.serverSnapshot) : false)}
          title={draft.mode === "existing" && (draft.serverSnapshot ? draftPublishEnabled(draft.serverSnapshot) : false) ? "Cannot be disabled" : undefined}
          aria-pressed={draft.draft_publish}
          onClick={() => {
            clearBanner();
            setTypeDraft((d) => ({ ...d, draft_publish: !d.draft_publish }));
          }}
        >
          <span className="rs-toggle-knob" />
        </button>
      </div>

      <div className="rs-schema">
        <div className="rs-schema-head"><span>Field</span><span>Type</span><span></span></div>
        {draft.fields.map((f) => (
          <FieldRow
            key={f.id}
            field={f}
            onEdit={() => { if (!isManaged) editField(f); }}
            onRemove={() => { if (!isManaged) removeField(f); }}
          />
        ))}
        <button className="rs-schema-add" onClick={addField} disabled={isManaged}>
          <Icons.plus size={16} /> Add another field to this collection type
        </button>
      </div>

      {confirming && type && (
        <DeleteTypeModal
          typeName={type}
          deleting={deleting}
          error={delBanner}
          onConfirm={doDelete}
          onClose={() => { setConfirming(false); setDelBanner(null); }}
        />
      )}
      {modal?.step === "pick" && (
        <FieldPicker
          typeDisplay={draft.display_name || draft.name}
          isFirst={draft.fields.length === 0}
          onPick={pickKind}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.step === "config" && (
        <FieldConfigModal
          initial={modal.field}
          isNew={modal.isNew}
          typeNames={allTypes.data?.map((t) => t.name) ?? []}
          existingNames={draft.fields.filter((f) => f.id !== modal.field.id).map((f) => f.name)}
          components={allComponents.data ?? []}
          lockedEnumValues={lockedEnum(modal.field)}
          onSave={saveField}
          onBack={modal.isNew ? () => setModal({ step: "pick" }) : undefined}
          onClose={() => setModal(null)}
        />
      )}
    </div>
  );
}
