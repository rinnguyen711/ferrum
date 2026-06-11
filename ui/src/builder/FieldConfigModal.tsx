import { useEffect, useRef, useState } from "react";
import { Icons } from "../components/icons";
import { Notice } from "../components/ui";
import { fieldLabel, type Cardinality, type DraftField } from "./draftModel";
import { EnumEditor } from "./EnumEditor";
import type { Component } from "../api/types";

const CARDINALITIES: [Cardinality, string][] = [
  ["many_to_one", "many → one"],
  ["one_to_one", "one ↔ one"],
  ["many_to_many", "many ↔ many"],
];

export function FieldConfigModal({
  initial,
  isNew,
  typeNames,
  components,
  lockedEnumValues,
  onSave,
  onBack,
  onClose,
}: {
  initial: DraftField;
  isNew: boolean;
  typeNames: string[];
  components: Component[];
  lockedEnumValues: string[];
  onSave: (field: DraftField) => void;
  onBack?: () => void;
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

  // many_to_many + required is rejected by the server; media is never required.
  const requiredBlocked =
    (field.kind === "relation" && field.cardinality === "many_to_many") ||
    field.kind === "media";

  const save = () => {
    if (!field.name.trim()) { setErr("A field name is required."); setTab("basic"); return; }
    if (field.kind === "component" && !field.componentUid.trim()) {
      setErr("A component uid is required (e.g. shared.hero_block).");
      return;
    }
    const out = { ...field, name: field.name.trim() };
    if (requiredBlocked) out.required = false;
    onSave(out);
  };

  const I = Icons[field.kind === "relation" ? "relation" : field.kind === "media" ? "image" : "type"];

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
            <span className="rs-modal-eyebrow">
              {(isNew ? "Add a field" : "Edit field")} · {fieldLabel(field.kind)}
            </span>
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
          {err && <Notice>{err}</Notice>}

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

              {field.kind === "media" && (
                <div className="rs-field">
                  <div className="rs-field-label"><label>Selection</label></div>
                  <div className="rs-setting-row">
                    <div className="rs-setting-meta">
                      <strong>Allow multiple assets</strong>
                      <span>Pick a gallery of assets instead of a single one.</span>
                    </div>
                    <Toggle
                      on={field.mediaMultiple}
                      disabled={locked}
                      onChange={(v) => set({ mediaMultiple: v })}
                    />
                  </div>
                </div>
              )}

              {field.kind === "component" && (
                <>
                  <div className="rs-field">
                    <div className="rs-field-label"><label>Component</label></div>
                    {components.length > 0 ? (
                      <select
                        className="rs-input"
                        value={field.componentUid ?? ""}
                        disabled={locked}
                        onChange={(e) => { set({ componentUid: e.target.value }); setErr(null); }}
                      >
                        <option value="">Select a component…</option>
                        {components.map((c) => (
                          <option key={c.uid} value={c.uid}>{c.display_name} ({c.uid})</option>
                        ))}
                      </select>
                    ) : (
                      <input
                        className="rs-input"
                        placeholder="e.g. shared.hero_block"
                        value={field.componentUid ?? ""}
                        onChange={(e) => { set({ componentUid: e.target.value }); setErr(null); }}
                        disabled={locked}
                      />
                    )}
                  </div>
                  <div className="rs-setting-row">
                    <div className="rs-setting-meta">
                      <strong>Repeatable</strong>
                      <span>Store an array of this component instead of a single instance.</span>
                    </div>
                    <Toggle
                      on={field.componentMultiple}
                      disabled={locked}
                      onChange={(v) => set({ componentMultiple: v })}
                    />
                  </div>
                </>
              )}
            </div>
          )}

          {tab === "advanced" && (
            <div className="rs-fields">
              <div className="rs-setting-row">
                <div className="rs-setting-meta">
                  <strong>Required field</strong>
                  <span>
                    {requiredBlocked
                      ? (field.kind === "media"
                          ? "Media fields cannot be required."
                          : "Many-to-many relations cannot be required.")
                      : "The entry can't be saved while this is empty."}
                  </span>
                </div>
                <Toggle
                  on={field.required && !requiredBlocked}
                  disabled={locked || requiredBlocked}
                  onChange={(v) => set({ required: v })}
                />
              </div>
              <div className="rs-setting-row">
                <div className="rs-setting-meta">
                  <strong>Unique field</strong>
                  <span>No two entries may share the same value.</span>
                </div>
                <Toggle on={field.unique} disabled={locked} onChange={(v) => set({ unique: v })} />
              </div>
              <div className="rs-setting-row">
                <div className="rs-setting-meta">
                  <strong>Private field</strong>
                  <span>Hidden from the public API response.</span>
                </div>
                <Toggle on={field.isPrivate} onChange={(v) => set({ isPrivate: v })} />
              </div>
              <div className="rs-field">
                <div className="rs-field-label">
                  <label>Default value</label>
                  <span className="rs-field-hint">pre-filled when a new entry is created</span>
                </div>
                <input
                  className="rs-input rs-mono"
                  placeholder={field.kind === "boolean" ? "true / false" : "leave empty for none"}
                  value={field.defaultValue}
                  onChange={(e) => set({ defaultValue: e.target.value })}
                />
              </div>
            </div>
          )}
        </div>

        <div className="rs-modal-foot">
          {onBack && (
            <button className="rs-btn rs-btn--ghost" onClick={onBack}>
              <Icons.chevLeft size={15} /> Back
            </button>
          )}
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <div className="rs-spacer" />
          <button className="rs-btn rs-btn--primary" onClick={save}>
            <Icons.check size={15} /> {isNew ? "Add field" : "Save changes"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Toggle({
  on,
  disabled,
  onChange,
}: {
  on: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      className={"rs-toggle" + (on ? " is-on" : "")}
      disabled={disabled}
      aria-pressed={on}
      onClick={() => onChange(!on)}
    >
      <span className="rs-toggle-knob" />
    </button>
  );
}
