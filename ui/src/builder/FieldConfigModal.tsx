import { useEffect, useRef, useState } from "react";
import { Icons } from "../components/icons";
import type { FieldKind } from "../api/types";
import { KINDS, type Cardinality, type DraftField } from "./draftModel";
import { EnumEditor } from "./EnumEditor";

const CARDINALITIES: [Cardinality, string][] = [
  ["many_to_one", "many → one"],
  ["one_to_one", "one ↔ one"],
  ["many_to_many", "many ↔ many"],
];

export function FieldConfigModal({
  initial,
  isNew,
  typeNames,
  lockedEnumValues,
  onSave,
  onClose,
}: {
  initial: DraftField;
  isNew: boolean;             // adding a brand-new field (vs editing existing row)
  typeNames: string[];
  lockedEnumValues: string[]; // existing enum values that cannot be removed
  onSave: (field: DraftField) => void;
  onClose: () => void;
}) {
  const locked = initial.origin === "existing";
  const [tab, setTab] = useState<"basic" | "advanced">("basic");
  const [field, setField] = useState<DraftField>(initial);
  const [err, setErr] = useState<string | null>(null);

  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onCloseRef.current(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const set = (patch: Partial<DraftField>) => setField((f) => ({ ...f, ...patch }));

  // many_to_many + required is rejected by the server.
  const m2mRequiredBlocked = field.kind === "relation" && field.cardinality === "many_to_many";

  const save = () => {
    if (!field.name.trim()) { setErr("A field name is required."); setTab("basic"); return; }
    const out = { ...field, name: field.name.trim() };
    if (m2mRequiredBlocked) out.required = false;
    onSave(out);
  };

  const I = Icons[field.kind === "relation" ? "relation" : "type"];

  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div
        className="rs-modal rs-modal--wide"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="rs-modal-head">
          <div className="rs-modal-icon"><I size={18} /></div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">{isNew ? "Add a field" : "Edit field"}</span>
            <h2>{field.name || "Untitled field"}</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose}><Icons.x size={18} /></button>
        </div>

        <div className="rs-modal-tabs">
          <button className={"rs-etab" + (tab === "basic" ? " is-active" : "")} onClick={() => setTab("basic")}>
            Basic settings
          </button>
          <button className={"rs-etab" + (tab === "advanced" ? " is-active" : "")} onClick={() => setTab("advanced")}>
            Advanced settings
          </button>
        </div>

        <div className="rs-modal-body">
          {err && <div className="rs-login-error" style={{ marginBottom: 12 }}>{err}</div>}

          {tab === "basic" && (
            <div className="rs-fields">
              <div className="rs-field">
                <div className="rs-field-label"><label>Name</label></div>
                <input
                  className="rs-input rs-mono"
                  placeholder="field_name"
                  value={field.name}
                  disabled={locked}
                  onChange={(e) => { set({ name: e.target.value }); setErr(null); }}
                />
              </div>

              <div className="rs-field">
                <div className="rs-field-label"><label>Type</label></div>
                <select
                  className="rs-input"
                  value={field.kind}
                  disabled={locked}
                  onChange={(e) => set({ kind: e.target.value as FieldKind })}
                >
                  {KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
                </select>
              </div>

              {field.kind === "relation" && (
                <>
                  <div className="rs-field">
                    <div className="rs-field-label"><label>Target type</label></div>
                    <select
                      className="rs-input"
                      value={field.target}
                      disabled={locked}
                      onChange={(e) => set({ target: e.target.value })}
                    >
                      <option value="">target type…</option>
                      {typeNames.map((n) => <option key={n} value={n}>{n}</option>)}
                    </select>
                  </div>
                  <div className="rs-field">
                    <div className="rs-field-label"><label>Relation</label></div>
                    <div className="rs-rel-types">
                      {CARDINALITIES.map(([c, label]) => (
                        <button
                          key={c}
                          className={"rs-rel-btn" + (field.cardinality === c ? " is-on" : "")}
                          disabled={locked}
                          onClick={() => set({ cardinality: c })}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>
                  <div className="rs-field">
                    <div className="rs-field-label">
                      <label>Inverse</label>
                      <span className="rs-field-hint">optional</span>
                    </div>
                    <input
                      className="rs-input rs-mono"
                      placeholder="inverse"
                      value={field.inverse}
                      disabled={locked}
                      onChange={(e) => set({ inverse: e.target.value })}
                    />
                  </div>
                </>
              )}

              {field.kind === "enum" && (
                <div className="rs-field">
                  <div className="rs-field-label"><label>Values</label></div>
                  <EnumEditor
                    values={field.enumValues}
                    lockedValues={locked ? lockedEnumValues : []}
                    onChange={(enumValues) => set({ enumValues })}
                  />
                </div>
              )}
            </div>
          )}

          {tab === "advanced" && (
            <div className="rs-fields">
              <div className="rs-setting-row">
                <div className="rs-setting-meta">
                  <strong>Required field</strong>
                  <span>
                    {m2mRequiredBlocked
                      ? "Many-to-many relations cannot be required."
                      : "The entry can't be saved while this is empty."}
                  </span>
                </div>
                <label className="rs-checkbox">
                  <input
                    type="checkbox"
                    checked={field.required && !m2mRequiredBlocked}
                    disabled={locked || m2mRequiredBlocked}
                    onChange={(e) => set({ required: e.target.checked })}
                  />
                </label>
              </div>
              <div className="rs-setting-row">
                <div className="rs-setting-meta">
                  <strong>Unique field</strong>
                  <span>No two entries may share the same value.</span>
                </div>
                <label className="rs-checkbox">
                  <input
                    type="checkbox"
                    checked={field.unique}
                    disabled={locked}
                    onChange={(e) => set({ unique: e.target.checked })}
                  />
                </label>
              </div>
            </div>
          )}
        </div>

        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <button className="rs-btn rs-btn--primary" onClick={save}>
            <Icons.check size={15} /> {isNew ? "Add field" : "Save changes"}
          </button>
        </div>
      </div>
    </div>
  );
}
