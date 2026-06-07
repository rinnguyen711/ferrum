import { useEffect, useRef } from "react";
import { Icons } from "../components/icons";

export type ColumnDef = { key: string; label: string };

export function FieldsMenu({
  columns,
  visible,
  lockedKey,
  onToggle,
  onReset,
  onClose,
}: {
  columns: ColumnDef[];
  visible: Record<string, boolean>;
  lockedKey: string | undefined;
  onToggle: (key: string) => void;
  onReset: () => void;
  onClose: () => void;
}) {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onCloseRef.current(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const shown = columns.filter((c) => c.key === lockedKey || visible[c.key]).length;

  return (
    <>
      <div className="rs-pop-backdrop" onClick={onClose} />
      <div className="rs-pop rs-fieldsmenu" role="dialog" aria-label="Displayed fields">
        <div className="rs-pop-head">
          <span className="rs-pop-title">Displayed fields</span>
          <button className="rs-link-btn" onClick={onReset}>Reset</button>
        </div>
        <div className="rs-pop-body">
          {columns.map((c) => {
            const locked = c.key === lockedKey;
            const on = locked || !!visible[c.key];
            return (
              <button
                key={c.key}
                className={"rs-fieldsmenu-row" + (locked ? " is-locked" : "")}
                onClick={() => !locked && onToggle(c.key)}
                disabled={locked}
                type="button"
              >
                <span className={"rs-check" + (on ? " is-on" : "") + (locked ? " is-locked" : "")}>
                  {on && <Icons.check size={13} />}
                </span>
                <span className="rs-fieldsmenu-label">{c.label}</span>
                {locked && <Icons.lock size={14} className="rs-fieldsmenu-lock" />}
              </button>
            );
          })}
        </div>
        <div className="rs-pop-foot">{shown} of {columns.length} shown</div>
      </div>
    </>
  );
}
