import { useState } from "react";

export function EnumEditor({
  values,
  lockedValues,
  onChange,
}: {
  values: string[];
  lockedValues: string[];   // existing values that cannot be removed
  onChange: (values: string[]) => void;
}) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const v = draft.trim();
    if (v && !values.includes(v)) onChange([...values, v]);
    setDraft("");
  };
  const locked = new Set(lockedValues);
  return (
    <div className="rs-fieldrow-sub">
      <div className="rs-chips rs-chips--wrap">
        {values.map((v) => (
          <span key={v} className="rs-chip">
            {v}
            {!locked.has(v) && (
              <button
                className="rs-chip-x"
                onClick={() => onChange(values.filter((x) => x !== v))}
              >
                ×
              </button>
            )}
          </span>
        ))}
      </div>
      <div className="rs-input-affix">
        <input
          className="rs-input rs-mono"
          placeholder="enum value"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={add}>
          Add
        </button>
      </div>
    </div>
  );
}
