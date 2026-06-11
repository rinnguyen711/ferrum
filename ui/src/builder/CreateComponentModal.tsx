import { useState } from "react";
import { Notice } from "../components/ui";
import { createComponent } from "../api/endpoints";
import type { Component } from "../api/types";
import { deriveApiId } from "./draftModel";

export function CreateComponentModal({
  existingComponents,
  onClose,
  onCreated,
}: {
  existingComponents: Component[];
  onClose: () => void;
  onCreated: (uid: string) => void;
}) {
  const categories = Array.from(
    new Set(existingComponents.map((c) => c.uid.split(".")[0])),
  ).sort();

  const [name, setName] = useState("");
  const [category, setCategory] = useState(categories[0] ?? "");
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const effectiveCategory = category.trim();
  const apiId = deriveApiId(name);
  const uid = effectiveCategory && apiId ? `${effectiveCategory}.${apiId}` : "";

  const submit = async () => {
    if (!name.trim()) return setErr("Name is required.");
    if (!effectiveCategory) return setErr("Category is required.");
    if (!/^[a-z][a-z0-9_]*$/.test(effectiveCategory)) {
      return setErr("Category must be lowercase letters, digits, underscores.");
    }
    if (!apiId) return setErr("Name must contain at least one letter.");
    setSaving(true);
    setErr(null);
    try {
      const c = await createComponent({ uid, display_name: name.trim(), fields: [] });
      onCreated(c.uid);
    } catch (e: unknown) {
      setErr((e as Error)?.message ?? "Create failed.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-modal-backdrop" onClick={() => { if (!saving) onClose(); }}>
      <div
        className="rs-modal"
        role="dialog"
        aria-modal="true"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape" && !saving) onClose();
          if (e.key === "Enter" && !saving) submit();
        }}
      >
        <div className="rs-modal-head">
          <h2>Create a component</h2>
        </div>

        <div className="rs-modal-body">
          {err && <Notice>{err}</Notice>}

          <div className="rs-field">
            <div className="rs-field-label"><label>Name</label></div>
            <input
              className="rs-input"
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Hero Block"
              disabled={saving}
            />
          </div>

          <div className="rs-field">
            <div className="rs-field-label"><label>Category</label></div>
            <input
              className="rs-input rs-mono"
              list="component-categories"
              value={category}
              onChange={(e) => setCategory(e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, ""))}
              disabled={saving}
            />
            <datalist id="component-categories">
              {categories.map((cat) => <option key={cat} value={cat} />)}
            </datalist>
          </div>

          {uid && (
            <p className="rs-field-hint rs-mono" style={{ marginTop: 4 }}>
              UID: {uid}
            </p>
          )}
        </div>

        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose} disabled={saving}>Cancel</button>
          <button className="rs-btn rs-btn--primary" onClick={submit} disabled={saving || !uid}>
            {saving ? "Creating…" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
