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

/** Editor top bar: back button + title (+ optional status) + actions. */
export function EditorBar({
  onBack,
  title,
  status,
  actions,
}: {
  onBack: () => void;
  title: ReactNode;
  status?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="rs-editor-bar">
      <button className="rs-back" onClick={onBack} aria-label="Back">
        <Icons.arrowLeft size={18} />
      </button>
      <div className="rs-editor-titlewrap">
        <h1>{title}</h1>
        {status}
      </div>
      {actions && <div className="rs-editor-actions">{actions}</div>}
    </div>
  );
}
