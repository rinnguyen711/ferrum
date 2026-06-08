import { useEffect, useRef, useState } from "react";
import { Icons } from "../components/icons";
import type { FieldKind } from "../api/types";
import { FIELD_CARDS } from "./draftModel";

export function FieldPicker({
  typeDisplay,
  isFirst,
  onPick,
  onClose,
}: {
  typeDisplay: string;
  isFirst: boolean;
  onPick: (kind: FieldKind) => void;
  onClose: () => void;
}) {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onCloseRef.current(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const filtered = q
    ? FIELD_CARDS.filter(
        (ft) => ft.label.toLowerCase().includes(q) || ft.desc.toLowerCase().includes(q),
      )
    : FIELD_CARDS;

  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div
        className="rs-modal rs-modal--wide"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="rs-modal-head">
          <div className="rs-modal-icon"><Icons.layers size={18} /></div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">{typeDisplay}</span>
            <h2>{isFirst ? "Add your first field" : "Select a field type"}</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose}><Icons.x size={18} /></button>
        </div>

        <div className="rs-modal-body">
          <div className="rs-panel-search" style={{ marginBottom: 12 }}>
            <Icons.search size={15} />
            <input
              autoFocus
              placeholder="Search field types…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          {filtered.length === 0 ? (
            <p style={{ fontSize: 13, color: "var(--text-muted)", margin: 0 }}>
              No field types match.
            </p>
          ) : (
            <div className="rs-fieldgrid">
              {filtered.map((ft) => {
                const I = Icons[ft.icon];
                return (
                  <button
                    key={ft.kind}
                    className="rs-fieldgrid-item"
                    onClick={() => onPick(ft.kind)}
                  >
                    <div className="rs-fieldgrid-icon"><I size={20} /></div>
                    <div className="rs-fieldgrid-text">
                      <strong>{ft.label}</strong>
                      <span>{ft.desc}</span>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
