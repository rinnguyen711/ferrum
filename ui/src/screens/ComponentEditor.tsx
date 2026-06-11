import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Notice, LoadingState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { getComponent, updateComponent, deleteComponent, listContentTypes, listComponents } from "../api/endpoints";
import type { FieldKind } from "../api/types";
import { relationMeta, enumValues, mediaMeta } from "../api/types";
import { ApiError } from "../api/client";
import { useBuilderDraft } from "../builder/BuilderDraftContext";
import { FieldRow } from "../builder/FieldRow";
import { FieldPicker } from "../builder/FieldPicker";
import { FieldConfigModal } from "../builder/FieldConfigModal";
import { blankField, draftFieldToField, type Cardinality, type DraftField } from "../builder/draftModel";

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
  const { bumpNonce } = useBuilderDraft();

  const { data: component, loading, error: loadError } = useResource(
    () => (uid ? getComponent(uid) : Promise.resolve(null)),
    [uid],
  );

  const allTypes = useResource(() => listContentTypes(), []);
  const allComponents = useResource(() => listComponents(), []);

  const [fields, setFields] = useState<DraftField[]>([]);
  const [displayName, setDisplayName] = useState("");
  const [banner, setBanner] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [delBanner, setDelBanner] = useState<string | null>(null);
  const [modal, setModal] = useState<FieldModal | null>(null);

  useEffect(() => {
    if (component) {
      setDisplayName(component.display_name);
      setFields(
        component.fields.map((f) => {
          const rel = relationMeta(f);
          return {
            ...blankField(f.kind),
            name: f.name,
            kind: f.kind,
            required: f.required,
            unique: f.unique,
            enumValues: enumValues(f),
            target: rel?.target ?? "",
            inverse: rel?.inverse ?? "",
            cardinality: (rel?.cardinality as Cardinality) ?? "many_to_one",
            mediaMultiple: mediaMeta(f)?.multiple ?? false,
            componentUid: (f.kind_meta as Record<string, unknown>)?.component as string ?? "",
            componentMultiple: (f.kind_meta as Record<string, unknown>)?.multiple === true,
            defaultValue: "",
            isPrivate: false,
            origin: "existing" as const,
          };
        }),
      );
    }
  }, [component]);

  useEffect(() => {
    setModal(null);
    setConfirming(false);
    setDeleting(false);
    setDelBanner(null);
    setBanner(null);
  }, [uid]);

  const saveFields = async () => {
    if (!uid) return;
    setSaving(true);
    setBanner(null);
    try {
      const wireFields = fields.map(draftFieldToField);
      await updateComponent(uid, { display_name: displayName, fields: wireFields });
      bumpNonce();
    } catch (e: unknown) {
      setBanner(e instanceof ApiError ? e.message : "Save failed.");
    } finally {
      setSaving(false);
    }
  };

  const doDelete = async () => {
    if (!uid) return;
    setDeleting(true);
    setDelBanner(null);
    try {
      await deleteComponent(uid);
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
    setFields((prev) => {
      const exists = prev.some((x) => x.id === f.id);
      return exists ? prev.map((x) => (x.id === f.id ? f : x)) : [...prev, f];
    });
    setModal(null);
  };

  const removeField = (f: DraftField) =>
    setFields((prev) => prev.filter((x) => x.id !== f.id));

  if (loading) return <LoadingState />;
  if (loadError || !component) return <div className="rs-empty">Component not found.</div>;

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <input
            className="rs-input rs-title-input"
            value={displayName}
            onChange={(e) => { setBanner(null); setDisplayName(e.target.value); }}
            placeholder="Display name"
          />
          <p className="rs-cm-sub rs-mono">
            component::{uid} · {fields.length} fields
          </p>
        </div>
        <div className="rs-editor-actions">
          <button
            className="rs-btn rs-btn--ghost rs-danger"
            onClick={() => setConfirming(true)}
          >
            Delete component
          </button>
          <button
            className="rs-btn rs-btn--primary"
            onClick={saveFields}
            disabled={saving}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>

      {banner && <Notice>{banner}</Notice>}

      <div className="rs-schema">
        <div className="rs-schema-head"><span>Field</span><span>Type</span><span></span></div>
        {fields.map((f) => (
          <FieldRow
            key={f.id}
            field={f}
            onEdit={() => setModal({ step: "config", field: f, isNew: false })}
            onRemove={() => removeField(f)}
          />
        ))}
        <button className="rs-schema-add" onClick={addField}>
          <Icons.plus size={16} /> Add another field to this component
        </button>
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
          typeDisplay={displayName || uid || ""}
          isFirst={fields.length === 0}
          onPick={pickKind}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.step === "config" && (
        <FieldConfigModal
          initial={modal.field}
          isNew={modal.isNew}
          typeNames={allTypes.data?.map((t) => t.name) ?? []}
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
