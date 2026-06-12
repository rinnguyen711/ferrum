import { useEffect, useRef } from "react";
import { Icons } from "../components/icons";
import { useResource } from "../hooks/useResource";
import { listEntries } from "../api/endpoints";
import type { ContentType, Field, FieldKind } from "../api/types";
import { enumValues, relationMeta } from "../api/types";
import { relationLabel } from "../util";

export type FilterRule = { id: string; field: string; op: string; value: string };

type EditorKind = "text" | "number" | "date" | "boolean" | "enum" | "relation";

const TEXT_KINDS: FieldKind[] = ["string", "text", "email", "url", "slug"];

type OpDef = [op: string, label: string];

const TEXT_OPS: OpDef[] = [
  ["$containsi", "contains"],
  ["$eq", "is"],
  ["$ne", "is not"],
  ["$startsWith", "starts with"],
  ["$endsWith", "ends with"],
];
const NUM_OPS: OpDef[] = [
  ["$eq", "="],
  ["$ne", "≠"],
  ["$gt", ">"],
  ["$lt", "<"],
  ["$gte", "≥"],
  ["$lte", "≤"],
];
const DATE_OPS: OpDef[] = [["$gte", "is after"], ["$lt", "is before"]];
const CHOICE_OPS: OpDef[] = [["$eq", "is"], ["$ne", "is not"]];
const BOOL_OPS: OpDef[] = [["$eq", "is"]];

/** Which value editor a field gets; null = not filterable (m2m, json, media…). */
export function editorKind(f: Field): EditorKind | null {
  if (TEXT_KINDS.includes(f.kind)) return "text";
  if (f.kind === "integer" || f.kind === "float") return "number";
  if (f.kind === "datetime") return "date";
  if (f.kind === "boolean") return "boolean";
  if (f.kind === "enum") return "enum";
  if (f.kind === "relation") {
    // m2m has no stored column — the backend rejects it as a filter target.
    const m = relationMeta(f);
    return m && m.cardinality !== "many_to_many" ? "relation" : null;
  }
  return null;
}

export function opsFor(f: Field): OpDef[] {
  switch (editorKind(f)) {
    case "text": return TEXT_OPS;
    case "number": return NUM_OPS;
    case "date": return DATE_OPS;
    case "boolean": return BOOL_OPS;
    case "enum":
    case "relation": return CHOICE_OPS;
    default: return [];
  }
}

export function filterableFields(ct: ContentType): Field[] {
  return ct.fields.filter((f) => editorKind(f) !== null);
}

export const isComplete = (r: FilterRule) => r.value !== "";

export function makeRule(f: Field): FilterRule {
  return {
    id: Math.random().toString(36).slice(2, 8),
    field: f.name,
    op: opsFor(f)[0][0],
    value: "",
  };
}

/** Complete rules → `filters[field][$op]=value` query pairs. */
export function serializeFilters(rules: FilterRule[], ct: ContentType): [string, string][] {
  return rules.filter(isComplete).map((r) => {
    const f = ct.fields.find((x) => x.name === r.field);
    // <input type="date"> yields YYYY-MM-DD; backend wants RFC3339.
    const v = f?.kind === "datetime" ? `${r.value}T00:00:00Z` : r.value;
    return [`filters[${r.field}][${r.op}]`, v] as [string, string];
  });
}

function RelationSelect({
  target,
  allTypes,
  value,
  onChange,
}: {
  target: string;
  allTypes: ContentType[] | null;
  value: string;
  onChange: (v: string) => void;
}) {
  const targetCt = allTypes?.find((t) => t.name === target);
  const entries = useResource(() => listEntries(target, { pageSize: 100 }), [target]);
  return (
    <select
      className="rs-fl-select rs-fl-val"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="">{entries.loading ? "Loading…" : "Select…"}</option>
      {(entries.data?.data ?? []).map((en) => (
        <option key={en.id} value={en.id}>{relationLabel(en, targetCt)}</option>
      ))}
    </select>
  );
}

function ValueEditor({
  field,
  allTypes,
  value,
  onChange,
}: {
  field: Field;
  allTypes: ContentType[] | null;
  value: string;
  onChange: (v: string) => void;
}) {
  switch (editorKind(field)) {
    case "number":
      return (
        <input
          className="rs-fl-input rs-fl-val"
          type="number"
          placeholder="value"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "date":
      return (
        <input
          className="rs-fl-input rs-fl-val"
          type="date"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "boolean":
      return (
        <select
          className="rs-fl-select rs-fl-val"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">Select…</option>
          <option value="true">Yes</option>
          <option value="false">No</option>
        </select>
      );
    case "enum":
      return (
        <select
          className="rs-fl-select rs-fl-val"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">Select…</option>
          {enumValues(field).map((v) => <option key={v} value={v}>{v}</option>)}
        </select>
      );
    case "relation": {
      const m = relationMeta(field);
      if (!m) return null;
      return (
        <RelationSelect target={m.target} allTypes={allTypes} value={value} onChange={onChange} />
      );
    }
    default:
      return (
        <input
          className="rs-fl-input rs-fl-val"
          type="text"
          placeholder="value"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
}

export function FiltersMenu({
  ct,
  allTypes,
  rules,
  setRules,
  onClose,
}: {
  ct: ContentType;
  allTypes: ContentType[] | null;
  rules: FilterRule[];
  setRules: (rules: FilterRule[]) => void;
  onClose: () => void;
}) {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onCloseRef.current(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const fields = filterableFields(ct);
  const fieldDef = (name: string) => fields.find((f) => f.name === name);
  const active = rules.filter(isComplete).length;

  const addRule = () => setRules([...rules, makeRule(fields[0])]);
  const removeRule = (id: string) => setRules(rules.filter((r) => r.id !== id));
  const clearAll = () => setRules([]);
  const updateRule = (id: string, patch: Partial<FilterRule>) =>
    setRules(rules.map((r) => {
      if (r.id !== id) return r;
      // Changing the field resets op + value to that field type's defaults.
      if (patch.field && patch.field !== r.field) {
        const def = fieldDef(patch.field);
        if (def) return { ...r, field: def.name, op: opsFor(def)[0][0], value: "" };
      }
      return { ...r, ...patch };
    }));

  return (
    <>
      <div className="rs-pop-backdrop" onClick={onClose} />
      <div className="rs-pop rs-pop--filters" role="dialog" aria-label="Filters">
        <div className="rs-pop-head">
          <span className="rs-pop-title">Filters</span>
          {rules.length > 0 && (
            <button className="rs-link-btn" onClick={clearAll}>Clear all</button>
          )}
        </div>
        <div className="rs-fl-body">
          {fields.length === 0 ? (
            <p className="rs-fl-empty">This type has no filterable fields.</p>
          ) : (
            <>
              {rules.length === 0 && (
                <p className="rs-fl-empty">
                  No filters applied. Narrow the list by adding a condition below.
                </p>
              )}
              {rules.map((r, i) => {
                const def = fieldDef(r.field);
                if (!def) return null;
                return (
                  <div key={r.id}>
                    {i > 0 && <div className="rs-fl-and"><span>AND</span></div>}
                    <div className="rs-fl-rule">
                      <div className="rs-fl-rule-top">
                        <select
                          className="rs-fl-select rs-fl-field"
                          value={r.field}
                          onChange={(e) => updateRule(r.id, { field: e.target.value })}
                        >
                          {fields.map((f) => (
                            <option key={f.name} value={f.name}>{f.name}</option>
                          ))}
                        </select>
                        <select
                          className="rs-fl-select rs-fl-op"
                          value={r.op}
                          onChange={(e) => updateRule(r.id, { op: e.target.value })}
                        >
                          {opsFor(def).map(([val, lbl]) => (
                            <option key={val} value={val}>{lbl}</option>
                          ))}
                        </select>
                        <button
                          className="rs-fl-rm"
                          onClick={() => removeRule(r.id)}
                          title="Remove condition"
                        >
                          <Icons.x size={14} />
                        </button>
                      </div>
                      <ValueEditor
                        field={def}
                        allTypes={allTypes}
                        value={r.value}
                        onChange={(value) => updateRule(r.id, { value })}
                      />
                    </div>
                  </div>
                );
              })}
              <button className="rs-fl-add" onClick={addRule}>
                <Icons.plus size={14} /> Add filter
              </button>
            </>
          )}
        </div>
        <div className="rs-pop-foot">
          {active === 0 ? "No active filters" : `${active} active filter${active > 1 ? "s" : ""}`}
        </div>
      </div>
    </>
  );
}
