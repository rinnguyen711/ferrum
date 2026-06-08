import { useState, useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Plus, Trash2 } from "lucide-react";
import { LoadingState, EmptyState, Notice } from "../components/ui";
import { useResource } from "../hooks/useResource";
import {
  listComponents, getComponent, createComponent, updateComponent, deleteComponent,
} from "../api/endpoints";
import type { Field } from "../api/types";
import { FieldRow } from "../builder/FieldRow";
import { FieldConfigModal } from "../builder/FieldConfigModal";
import { blankField } from "../builder/draftModel";
import type { DraftField } from "../builder/draftModel";

export function ComponentBuilder() {
  const { uid } = useParams<{ uid: string }>();
  const navigate = useNavigate();

  const all = useResource(() => listComponents(), []);
  const selected = useResource(
    () => (uid && uid !== "new" ? getComponent(uid) : Promise.resolve(null)),
    [uid],
  );

  const [fields, setFields] = useState<DraftField[]>([]);
  const [displayName, setDisplayName] = useState("");
  const [newUid, setNewUid] = useState("");
  const [adding, setAdding] = useState(false);
  const [editingField, setEditingField] = useState<DraftField | null>(null);
  const [saving, setSaving] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);
  const isNew = !uid || uid === "new";

  useEffect(() => {
    if (selected.data) {
      setDisplayName(selected.data.display_name);
      setFields(
        selected.data.fields.map((f) => ({
          ...blankField(f.kind),
          name: f.name,
          required: f.required,
          unique: f.unique,
          origin: "existing" as const,
        }))
      );
    } else if (isNew) {
      setDisplayName("");
      setFields([]);
      setNewUid("");
    }
  }, [selected.data, isNew]);

  const save = async () => {
    setSaving(true);
    setBanner(null);
    try {
      const wireFields: Field[] = fields.map((d) => ({
        name: d.name,
        kind: d.kind,
        required: d.required,
        unique: d.unique,
        default: null,
        kind_meta: d.kind === "enum" ? { values: d.enumValues } : {},
      }));
      if (isNew) {
        const c = await createComponent({ uid: newUid, display_name: displayName, fields: wireFields });
        all.refetch();
        navigate(`/components/${encodeURIComponent(c.uid)}`);
      } else if (uid) {
        await updateComponent(uid, { display_name: displayName, fields: wireFields });
        all.refetch();
      }
    } catch (e: unknown) {
      setBanner((e as Error)?.message ?? "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!uid || !confirm(`Delete component "${uid}"?`)) return;
    try {
      await deleteComponent(uid);
      all.refetch();
      navigate("/components");
    } catch (e: unknown) {
      setBanner((e as Error)?.message ?? "Delete failed");
    }
  };

  return (
    <div style={{ display: "flex", height: "100%", overflow: "hidden" }}>
      <div style={{ width: 240, borderRight: "1px solid var(--rs-border)", overflowY: "auto", padding: "16px 0" }}>
        <div style={{ padding: "0 16px 12px" }}>
          <button
            className="rs-btn rs-btn--primary rs-btn--sm"
            style={{ width: "100%" }}
            onClick={() => navigate("/components/new")}
          >
            <Plus size={14} /> New Component
          </button>
        </div>
        {all.loading && <LoadingState />}
        {all.data?.map((c) => (
          <button
            key={c.uid}
            onClick={() => navigate(`/components/${encodeURIComponent(c.uid)}`)}
            style={{
              width: "100%", textAlign: "left", padding: "8px 16px",
              background: c.uid === uid ? "var(--rs-surface-raised)" : "none",
              border: "none", cursor: "pointer",
            }}
          >
            <div style={{ fontWeight: 500, fontSize: 13 }}>{c.display_name}</div>
            <div style={{ fontSize: 11, color: "var(--rs-fg-muted)" }}>{c.uid}</div>
          </button>
        ))}
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: 24 }}>
        {!uid && !isNew && (
          <EmptyState>Select a component or create a new one.</EmptyState>
        )}
        {(uid || isNew) && (
          <>
            {banner && <Notice>{banner}</Notice>}
            {isNew && (
              <div className="rs-field" style={{ marginBottom: 16 }}>
                <div className="rs-field-label">
                  <label>UID</label>
                  <span className="rs-field-hint">e.g. shared.hero_block</span>
                </div>
                <input
                  className="rs-input"
                  value={newUid}
                  onChange={(e) => setNewUid(e.target.value)}
                  placeholder="category.name"
                />
              </div>
            )}
            <div className="rs-field" style={{ marginBottom: 16 }}>
              <div className="rs-field-label"><label>Display Name</label></div>
              <input
                className="rs-input"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
              />
            </div>

            <div style={{ marginBottom: 12, fontWeight: 600 }}>Fields</div>
            <div className="rs-fields">
              {fields.map((f) => (
                <FieldRow
                  key={f.id}
                  field={f}
                  onEdit={() => { setAdding(false); setEditingField(f); }}
                  onRemove={() => setFields((prev) => prev.filter((x) => x.id !== f.id))}
                />
              ))}
            </div>
            <button
              className="rs-btn rs-btn--ghost rs-btn--sm"
              style={{ marginTop: 8 }}
              onClick={() => { setAdding(true); setEditingField(blankField()); }}
            >
              <Plus size={13} /> Add field
            </button>

            <div style={{ marginTop: 24, display: "flex", gap: 8 }}>
              <button className="rs-btn rs-btn--primary" onClick={save} disabled={saving}>
                {saving ? "Saving…" : isNew ? "Create" : "Save"}
              </button>
              {!isNew && uid && (
                <button className="rs-btn rs-btn--ghost" onClick={handleDelete}>
                  <Trash2 size={14} /> Delete
                </button>
              )}
            </div>
          </>
        )}
      </div>

      {editingField && (
        <FieldConfigModal
          initial={editingField}
          isNew={adding}
          typeNames={[]}
          lockedEnumValues={[]}
          onSave={(f) => {
            if (adding) {
              setFields((prev) => [...prev, { ...f, origin: "new" }]);
            } else {
              setFields((prev) => prev.map((x) => (x.id === f.id ? f : x)));
            }
            setEditingField(null);
            setAdding(false);
          }}
          onBack={() => { setEditingField(null); setAdding(false); }}
          onClose={() => { setEditingField(null); setAdding(false); }}
        />
      )}
    </div>
  );
}
