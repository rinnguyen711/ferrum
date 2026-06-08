import { useEffect, useRef, useState } from "react";
import { Notice } from "../components/ui";
import { createComponent } from "../api/endpoints";

export function CreateComponentModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (uid: string) => void;
}) {
  const [displayName, setDisplayName] = useState("");
  const [uid, setUid] = useState("");
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const savingRef = useRef(saving);
  savingRef.current = saving;

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !savingRef.current) onCloseRef.current();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const submit = async () => {
    if (!displayName.trim()) return setErr("Display name is required.");
    if (!uid.trim()) return setErr("UID is required.");
    if (!/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/.test(uid.trim())) {
      return setErr("UID must be category.name (e.g. shared.quote).");
    }
    setSaving(true);
    setErr(null);
    try {
      const c = await createComponent({ uid: uid.trim(), display_name: displayName.trim(), fields: [] });
      onCreated(c.uid);
    } catch (e: unknown) {
      setErr((e as Error)?.message ?? "Create failed.");
      setSaving(false);
    }
  };

  return (
    <div
      className="rs-modal-backdrop"
      onClick={() => { if (!saving) onClose(); }}
    >
      <div
        className="rs-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="rs-modal-head">
          <h2>Create a component</h2>
        </div>
        <div className="rs-modal-body">
          {err && <Notice>{err}</Notice>}
          <div className="rs-field">
            <div className="rs-field-label"><label>Display name</label></div>
            <input
              className="rs-input"
              autoFocus
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Quote"
              disabled={saving}
            />
          </div>
          <div className="rs-field">
            <div className="rs-field-label">
              <label>UID</label>
              <span className="rs-field-hint">category.name — e.g. shared.quote</span>
            </div>
            <input
              className="rs-input rs-mono"
              value={uid}
              onChange={(e) => setUid(e.target.value)}
              placeholder="shared.quote"
              disabled={saving}
            />
          </div>
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose} disabled={saving}>
            Cancel
          </button>
          <button className="rs-btn rs-btn--primary" onClick={submit} disabled={saving}>
            {saving ? "Creating…" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
