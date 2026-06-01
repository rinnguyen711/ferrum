import type { PatchContentType } from "../api/types";

export function SaveConfirmModal({
  patch,
  saving,
  onConfirm,
  onCancel,
}: {
  patch: PatchContentType;
  saving: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="rs-modal-backdrop" onClick={onCancel}>
      <div
        className="rs-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onCancel();
        }}
      >
        <div className="rs-modal-head">
          <h2>Confirm schema changes</h2>
        </div>
        <div className="rs-modal-body">
          <div className="rs-login-error" style={{ marginBottom: 12 }}>
            Dropping a field deletes its column and all of its data. This cannot
            be undone.
          </div>
          <ul className="rs-change-list">
            {patch.drop_fields.map((f) => (
              <li key={"d" + f}><strong className="rs-danger">Drop</strong> {f}</li>
            ))}
            {patch.add_fields.map((f) => (
              <li key={"a" + f.name}><strong>Add</strong> {f.name} ({f.kind})</li>
            ))}
            {patch.extend_enum_values.map((e) => (
              <li key={"e" + e.field}>
                <strong>Extend enum</strong> {e.field}: +{e.append.join(", ")}
              </li>
            ))}
            {patch.display_name !== undefined && (
              <li><strong>Rename display</strong> → {patch.display_name}</li>
            )}
          </ul>
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onCancel} disabled={saving}>
            Cancel
          </button>
          <button
            className="rs-btn rs-btn--primary rs-danger"
            onClick={onConfirm}
            disabled={saving}
          >
            {saving ? "Saving…" : "Apply changes"}
          </button>
        </div>
      </div>
    </div>
  );
}
