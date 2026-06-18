import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Notice, LoadingState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { getComponent, deleteComponent, listContentTypes, listComponents } from "../api/endpoints";
import type { FieldKind } from "../api/types";
import { managedComponent } from "../api/types";
import { ApiError } from "../api/client";
import { useBuilderDraft } from "../builder/BuilderDraftContext";
import { FieldRow } from "../builder/FieldRow";
import { FieldPicker } from "../builder/FieldPicker";
import { FieldConfigModal } from "../builder/FieldConfigModal";
import { SaveBar } from "../builder/SaveBar";
import { blankField, type ComponentDraft, type DraftField } from "../builder/draftModel";

function DeleteComponentModal({
  uid,
  deleting,
  error,
  onConfirm,
  onClose,
}: {
  uid: string;
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
            <h2>Delete "{uid}"?</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose} disabled={deleting} aria-label="Close">
            <Icons.x size={18} />
          </button>
        </div>
        <div className="rs-modal-body">
          <p style={{ fontSize: 14, color: "var(--text-muted)", margin: 0 }}>
            This will permanently delete the <strong className="rs-mono">{uid}</strong> component. This cannot be undone.
          </p>
          {error && <div style={{ marginTop: 12 }}><Notice>{error}</Notice></div>}
        </div>
        <div className="rs-modal-foot" style={{ justifyContent: "space-between" }}>
          <button className="rs-btn rs-btn--ghost" onClick={onClose} disabled={deleting}>Cancel</button>
          <button
            className="rs-btn rs-btn--primary"
            onClick={onConfirm}
            disabled={deleting}
            style={{ background: "var(--danger)", borderColor: "var(--danger)", color: "#fff" }}
          >
            {deleting ? "Deleting…" : "Delete component"}
          </button>
        </div>
      </div>
    </div>
  );
}

type FieldModal =
  | { step: "pick" }
  | { step: "config"; field: DraftField; isNew: boolean };

export function ComponentEditor() {
  const { uid } = useParams<{ uid: string }>();
  const navigate = useNavigate();
  const {
    draft, banner, setDraft, clearBanner, loadExistingComponent, reset, bumpNonce,
  } = useBuilderDraft();

  const { data: component, loading, error: loadError } = useResource(
    () => (uid ? getComponent(uid) : Promise.resolve(null)),
    [uid],
  );

  const allTypes = useResource(() => listContentTypes(), []);
  const allComponents = useResource(() => listComponents(), []);

  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [delBanner, setDelBanner] = useState<string | null>(null);
  const [modal, setModal] = useState<FieldModal | null>(null);

  // Seed the shared builder draft from the loaded component (once per uid).
  useEffect(() => {
    if (!component) return;
    if (draft && draft.mode === "component" && draft.uid === component.uid) return;
    loadExistingComponent(component);
  }, [component, draft, loadExistingComponent]);

  useEffect(() => {
    setModal(null);
    setConfirming(false);
    setDeleting(false);
    setDelBanner(null);
  }, [uid]);

  // Narrowed setter — this editor only ever touches component drafts.
  const setCompDraft = (fn: (d: ComponentDraft) => ComponentDraft) =>
    setDraft((d) => (d.mode === "component" ? fn(d) : d));

  const doDelete = async () => {
    if (!uid) return;
    setDeleting(true);
    setDelBanner(null);
    try {
      await deleteComponent(uid);
      reset();
      bumpNonce();
      navigate("/builder");
    } catch (e: unknown) {
      setDelBanner(e instanceof ApiError ? e.message : "Delete failed.");
    } finally {
      setDeleting(false);
    }
  };

  const addField = () => setModal({ step: "pick" });
  const pickKind = (kind: FieldKind) =>
    setModal({ step: "config", field: blankField(kind), isNew: true });

  const saveField = (f: DraftField) => {
    setCompDraft((d) => {
      const exists = d.fields.some((x) => x.id === f.id);
      return exists
        ? { ...d, fields: d.fields.map((x) => (x.id === f.id ? f : x)) }
        : { ...d, fields: [...d.fields, f] };
    });
    setModal(null);
  };

  const removeField = (f: DraftField) =>
    setCompDraft((d) => ({ ...d, fields: d.fields.filter((x) => x.id !== f.id) }));

  // Field reorder — drag + Arrow keys on the grip. Mirrors SchemaEditor.
  const moveField = (from: number, to: number, len: number) => {
    if (to < 0 || to >= len || from === to) return;
    clearBanner();
    setCompDraft((d) => {
      const next = d.fields.slice();
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      return { ...d, fields: next };
    });
  };
  const dragSrc = useRef<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  if (loading) return <LoadingState />;
  if (loadError || !component) return <div className="rs-empty">Component not found.</div>;
  if (!draft || draft.mode !== "component" || draft.uid !== component.uid) return <LoadingState />;

  const isManaged = managedComponent(component);

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <input
            className="rs-input rs-title-input"
            value={draft.display_name}
            onChange={(e) => { clearBanner(); setCompDraft((d) => ({ ...d, display_name: e.target.value })); }}
            placeholder="Display name"
            disabled={isManaged}
          />
          <p className="rs-cm-sub rs-mono">
            component::{uid} · {draft.fields.length} fields
          </p>
        </div>
        <div className="rs-editor-actions">
          <button
            className="rs-btn rs-btn--ghost rs-danger"
            onClick={() => setConfirming(true)}
            disabled={isManaged}
          >
            Delete component
          </button>
        </div>
      </div>

      {isManaged && (
        <Notice tone="ok">
          Managed by a schema file — edit the TOML and restart to change this component.
        </Notice>
      )}
      <div role="alert" aria-live="assertive">
        {banner && <Notice>{banner}</Notice>}
      </div>

      <SaveBar disabled={isManaged} />

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
            <span>Add your first field to define this component's shape.</span>
          </div>
        ) : (
          draft.fields.map((f, i) => (
            <FieldRow
              key={f.id}
              field={f}
              index={i}
              count={draft.fields.length}
              reorderable={!isManaged}
              dragOver={dragOver === i}
              onEdit={() => { if (!isManaged) setModal({ step: "config", field: f, isNew: false }); }}
              onRemove={() => { if (!isManaged) removeField(f); }}
              onMove={(dir) => moveField(i, i + dir, draft.fields.length)}
              onDragStart={() => { dragSrc.current = i; }}
              onDragEnter={() => { if (dragSrc.current !== null && dragSrc.current !== i) setDragOver(i); }}
              onDragEnd={() => { setDragOver(null); dragSrc.current = null; }}
              onDrop={() => {
                const from = dragSrc.current;
                setDragOver(null);
                dragSrc.current = null;
                if (from !== null) moveField(from, i, draft.fields.length);
              }}
            />
          ))
        )}
        {!isManaged && (
          <button className="rs-schema-add" onClick={addField}>
            <Icons.plus size={16} aria-hidden="true" /> Add another field to this component
          </button>
        )}
      </div>

      {confirming && uid && (
        <DeleteComponentModal
          uid={uid}
          deleting={deleting}
          error={delBanner}
          onConfirm={doDelete}
          onClose={() => { setConfirming(false); setDelBanner(null); }}
        />
      )}
      {modal?.step === "pick" && (
        <FieldPicker
          typeDisplay={draft.display_name || uid || ""}
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
          lockedEnumValues={[]}
          onSave={saveField}
          onBack={modal.isNew ? () => setModal({ step: "pick" }) : undefined}
          onClose={() => setModal(null)}
        />
      )}
    </div>
  );
}
