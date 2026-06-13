import type { ReactNode } from "react";
import { Icons } from "./icons";

/** Inline error/ok banner. Owns its own bottom margin. */
export function Notice({
  tone = "error",
  children,
}: {
  tone?: "error" | "ok";
  children: ReactNode;
}) {
  return <div className={"rs-notice" + (tone === "ok" ? " rs-notice--ok" : "")}>{children}</div>;
}

/** Centered loading placeholder, consistent copy. */
export function LoadingState({ label = "Loading…" }: { label?: string }) {
  return <div className="rs-empty">{label}</div>;
}

/** Centered empty / error placeholder with optional action. */
export function EmptyState({ children }: { children: ReactNode }) {
  return <div className="rs-empty">{children}</div>;
}

/** Shimmer rows inside a table-wrap — first-load placeholder for list views,
 *  matching the side-panel skeletons rather than a bare "Loading…". */
export function TableSkeleton({ rows = 8 }: { rows?: number }) {
  return (
    <div className="rs-table-wrap" aria-busy="true">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="rs-skel" style={{ height: 18, margin: "14px 16px" }} />
      ))}
    </div>
  );
}

/** Canonical checkbox (button + check glyph). */
export function Checkbox({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      className={"rs-check" + (checked ? " is-on" : "")}
      onClick={onChange}
    >
      {checked && <Icons.check size={13} />}
    </button>
  );
}

/** Destructive-action confirm dialog. Replaces native window.confirm for
 *  deletes so the prompt stays inside the design language. */
export function ConfirmDialog({
  title,
  body,
  confirmLabel = "Delete",
  busy = false,
  onConfirm,
  onCancel,
}: {
  title: ReactNode;
  body?: ReactNode;
  confirmLabel?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="rs-modal-backdrop" onClick={() => { if (!busy) onCancel(); }}>
      <div
        className="rs-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 420 }}
      >
        <div className="rs-modal-head">
          <div className="rs-modal-ico rs-modal-ico--danger">
            <Icons.trash size={18} />
          </div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">Destructive action</span>
            <h2>{title}</h2>
          </div>
          <button className="rs-modal-x" onClick={onCancel} disabled={busy} aria-label="Close">
            <Icons.x size={18} />
          </button>
        </div>
        {body && (
          <div className="rs-modal-body">
            <p style={{ fontSize: 14, color: "var(--text-muted)", margin: 0 }}>{body}</p>
          </div>
        )}
        <div className="rs-modal-foot" style={{ justifyContent: "space-between" }}>
          <button className="rs-btn rs-btn--ghost" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
          <button className="rs-btn rs-btn--danger" onClick={onConfirm} disabled={busy}>
            {busy ? "Working…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Editor top bar: back button + title (+ optional status) + actions. */
export function EditorBar({
  onBack,
  title,
  status,
  actions,
}: {
  onBack?: () => void;
  title: ReactNode;
  status?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="rs-editor-bar">
      {onBack && (
        <button type="button" className="rs-back" onClick={onBack} aria-label="Back">
          <Icons.arrowLeft size={18} />
        </button>
      )}
      <div className="rs-editor-titlewrap">
        <h1>{title}</h1>
        {status}
      </div>
      {actions && <div className="rs-editor-actions">{actions}</div>}
    </div>
  );
}
