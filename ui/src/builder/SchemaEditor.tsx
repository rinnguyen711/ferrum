import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Notice, LoadingState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import {
  deleteContentType, getContentType, listContentTypes, listComponents,
} from "../api/endpoints";
import { ApiError } from "../api/client";
import { draftPublishEnabled, localizedEnabled, enumValues, managedType } from "../api/types";
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
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    cancelRef.current?.focus(); // initial focus on the non-destructive action
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !deleting) { onCloseRef.current(); return; }
      if (e.key !== "Tab") return;
      // Trap focus within the dialog.
      const f = dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      if (!f || f.length === 0) return;
      const first = f[0];
      const last = f[f.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    };
    window.addEventListener("keydown", h);
    return () => {
      window.removeEventListener("keydown", h);
      trigger?.focus(); // restore focus to the element that opened the dialog
    };
  }, [deleting]);

  return (
    <div className="rs-modal-backdrop" onClick={() => { if (!deleting) onClose(); }}>
      <div
        ref={dialogRef}
        className="rs-modal rs-modal--sm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="del-type-title"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="rs-modal-head">
          <div className="rs-modal-ico rs-modal-ico--danger">
            <Icons.trash size={18} aria-hidden="true" />
          </div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">Destructive action</span>
            <h2 id="del-type-title">Delete "{typeName}"?</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose} disabled={deleting} aria-label="Close">
            <Icons.x size={18} aria-hidden="true" />
          </button>
        </div>
        <div className="rs-modal-body">
          <p className="rs-modal-text">
            This will permanently drop the <strong className="rs-mono">{typeName}</strong> content type and <strong>all its entries</strong>. This cannot be undone.
          </p>
          {error && <div className="rs-modal-error" role="alert"><Notice>{error}</Notice></div>}
        </div>
        <div className="rs-modal-foot">
          <button ref={cancelRef} className="rs-btn rs-btn--ghost" onClick={onClose} disabled={deleting}>
            Cancel
          </button>
          <button
            className="rs-btn rs-btn--primary rs-btn--danger"
            onClick={onConfirm}
            disabled={deleting}
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

  // Field-reorder state — declared before the early return so the hook order
  // stays stable across renders (React error #310 otherwise).
  const dragSrc = useRef<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

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

  // Field reorder — drag (mouse) + Arrow keys on the grip (keyboard). Matches
  // the media-field drag idiom. Managed types are read-only, so no reorder.
  const reorderable = !isManaged;
  const moveField = (from: number, to: number) => {
    if (to < 0 || to >= draft.fields.length || from === to) return;
    clearBanner();
    setTypeDraft((d) => {
      const next = d.fields.slice();
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      return { ...d, fields: next };
    });
  };
  const onDrop = (to: number) => {
    const from = dragSrc.current;
    setDragOver(null);
    dragSrc.current = null;
    if (from !== null) moveField(from, to);
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
            disabled={isManaged}
          />
          <p className="rs-cm-sub rs-mono">
            api::{draft.name}.{draft.name} · {draft.fields.length} fields · collection type
            {draft.mode === "new" ? " · unsaved" : ""}
          </p>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" disabled title="Coming soon" aria-label="Preview API (coming soon)">
            <Icons.eye size={15} aria-hidden="true" /> Preview API
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
      <div role="alert" aria-live="assertive">
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
      </div>

      <SaveBar disabled={isManaged} />

      <div className="rs-setting-row" style={{ marginBottom: 16 }}>
        <div className="rs-setting-meta">
          <strong>Enable Draft &amp; Publish</strong>
          <span>Entries go through a draft state before being published.</span>
        </div>
        <button
          type="button"
          role="switch"
          aria-label="Enable Draft & Publish"
          className={"rs-toggle" + (draft.draft_publish ? " is-on" : "")}
          disabled={isManaged || (draft.mode === "existing" && (draft.serverSnapshot ? draftPublishEnabled(draft.serverSnapshot) : false))}
          title={isManaged ? "Managed by a schema file" : (draft.mode === "existing" && (draft.serverSnapshot ? draftPublishEnabled(draft.serverSnapshot) : false) ? "Cannot be disabled" : undefined)}
          aria-checked={draft.draft_publish}
          onClick={() => {
            clearBanner();
            setTypeDraft((d) => ({ ...d, draft_publish: !d.draft_publish }));
          }}
        >
          <span className="rs-toggle-knob" />
        </button>
      </div>

      <div className="rs-setting-row" style={{ marginBottom: 16 }}>
        <div className="rs-setting-meta">
          <strong>Enable localization</strong>
          <span>Entries can have a separate version per locale. Existing entries become the default locale.</span>
        </div>
        <button
          type="button"
          role="switch"
          aria-label="Enable localization"
          className={"rs-toggle" + (draft.localized ? " is-on" : "")}
          disabled={isManaged || (draft.mode === "existing" && (draft.serverSnapshot ? localizedEnabled(draft.serverSnapshot) : false))}
          title={isManaged ? "Managed by a schema file" : (draft.mode === "existing" && (draft.serverSnapshot ? localizedEnabled(draft.serverSnapshot) : false) ? "Cannot be disabled" : undefined)}
          aria-checked={draft.localized}
          onClick={() => {
            clearBanner();
            setTypeDraft((d) => ({ ...d, localized: !d.localized }));
          }}
        >
          <span className="rs-toggle-knob" />
        </button>
      </div>

      <div className="rs-schema" role="table" aria-label="Fields">
        <div className="rs-schema-head" role="row">
          <span role="columnheader" className="rs-sr-only">Reorder</span>
          <span role="columnheader" className="rs-sr-only">Icon</span>
          <span role="columnheader">Field</span>
          <span role="columnheader">Type</span>
          <span role="columnheader" className="rs-sr-only">Actions</span>
        </div>
        {draft.fields.length === 0 ? (
          <div className="rs-schema-empty">
            <Icons.layers size={22} aria-hidden="true" />
            <strong>No fields yet</strong>
            <span>Add your first field to define this content type's shape.</span>
          </div>
        ) : (
          draft.fields.map((f, i) => (
            <FieldRow
              key={f.id}
              field={f}
              index={i}
              count={draft.fields.length}
              reorderable={reorderable}
              dragOver={dragOver === i}
              onEdit={() => { if (!isManaged) editField(f); }}
              onRemove={() => { if (!isManaged) removeField(f); }}
              onMove={(dir) => moveField(i, i + dir)}
              onDragStart={() => { dragSrc.current = i; }}
              onDragEnter={() => { if (dragSrc.current !== null && dragSrc.current !== i) setDragOver(i); }}
              onDragEnd={() => { setDragOver(null); dragSrc.current = null; }}
              onDrop={() => onDrop(i)}
            />
          ))
        )}
        {!isManaged && (
          <button className="rs-schema-add" onClick={addField}>
            <Icons.plus size={16} aria-hidden="true" /> Add another field to this collection type
          </button>
        )}
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
