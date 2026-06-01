import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useBuilderDraft } from "./BuilderDraftContext";
import { deriveApiId } from "./draftModel";

export function CreateTypeModal({ onClose }: { onClose: () => void }) {
  const { startNew } = useBuilderDraft();
  const navigate = useNavigate();
  const [display, setDisplay] = useState("");
  const [apiId, setApiId] = useState("");
  const [apiIdTouched, setApiIdTouched] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const onDisplayChange = (v: string) => {
    setDisplay(v);
    if (!apiIdTouched) setApiId(deriveApiId(v));
  };

  const cont = () => {
    if (!display.trim()) return setErr("Display name is required.");
    if (!apiId.trim()) return setErr("API ID is required.");
    startNew(apiId.trim(), display.trim());
    onClose();
    navigate("/builder/new");
  };

  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div
        className="rs-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className="rs-modal-head">
          <h2>Create a collection type</h2>
        </div>
        <div className="rs-modal-body">
          {err && <div className="rs-login-error" style={{ marginBottom: 12 }}>{err}</div>}
          <div className="rs-field">
            <div className="rs-field-label"><label>Display name</label></div>
            <input
              className="rs-input"
              autoFocus
              value={display}
              onChange={(e) => onDisplayChange(e.target.value)}
              placeholder="Article"
            />
          </div>
          <div className="rs-field">
            <div className="rs-field-label">
              <label>API ID</label>
              <span className="rs-field-hint">lowercase letters, digits, underscore</span>
            </div>
            <input
              className="rs-input rs-mono"
              value={apiId}
              onChange={(e) => {
                setApiIdTouched(true);
                setApiId(e.target.value);
              }}
              placeholder="article"
            />
          </div>
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <button className="rs-btn rs-btn--primary" onClick={cont}>Continue</button>
        </div>
      </div>
    </div>
  );
}
