import { Icons } from "../components/icons";
import { KINDS, type DraftField } from "./draftModel";
import type { FieldKind } from "../api/types";
import { EnumEditor } from "./EnumEditor";

export function FieldRow({
  field,
  error,
  typeNames,
  lockedEnumValues,
  staged,
  onChange,
  onRemove,
}: {
  field: DraftField;
  error?: string;
  typeNames: string[];
  lockedEnumValues: string[];     // existing enum values (only for origin="existing")
  staged: boolean;                // marked for drop (existing field removed)
  onChange: (patch: Partial<DraftField>) => void;
  onRemove: () => void;
}) {
  const locked = field.origin === "existing";
  return (
    <div className={"rs-fieldrow" + (staged ? " is-staged-drop" : "")}>
      <div className="rs-fieldrow-main">
        <input
          className="rs-input rs-mono"
          placeholder="field_name"
          value={field.name}
          disabled={locked}
          onChange={(e) => onChange({ name: e.target.value })}
        />
        <select
          className="rs-input"
          value={field.kind}
          disabled={locked}
          onChange={(e) => onChange({ kind: e.target.value as FieldKind })}
        >
          {KINDS.map((k) => (
            <option key={k} value={k}>{k}</option>
          ))}
        </select>
        <label className="rs-checkbox">
          <input
            type="checkbox"
            checked={field.required}
            disabled={locked}
            onChange={(e) => onChange({ required: e.target.checked })}
          />
          required
        </label>
        <label className="rs-checkbox">
          <input
            type="checkbox"
            checked={field.unique}
            disabled={locked}
            onChange={(e) => onChange({ unique: e.target.checked })}
          />
          unique
        </label>
        <button
          className={"rs-row-btn " + (staged ? "" : "rs-danger")}
          onClick={onRemove}
          title={staged ? "Keep field" : "Remove field"}
        >
          {staged ? <Icons.plus size={15} /> : <Icons.trash size={15} />}
        </button>
      </div>

      {field.kind === "relation" && (
        <div className="rs-fieldrow-sub">
          <select
            className="rs-input"
            value={field.target}
            disabled={locked}
            onChange={(e) => onChange({ target: e.target.value })}
          >
            <option value="">target type…</option>
            {typeNames.map((n) => (
              <option key={n} value={n}>{n}</option>
            ))}
          </select>
          <input
            className="rs-input rs-mono"
            placeholder="inverse (optional)"
            value={field.inverse}
            disabled={locked}
            onChange={(e) => onChange({ inverse: e.target.value })}
          />
        </div>
      )}

      {field.kind === "enum" && (
        <EnumEditor
          values={field.enumValues}
          lockedValues={locked ? lockedEnumValues : []}
          onChange={(enumValues) => onChange({ enumValues })}
        />
      )}

      {error && <div className="rs-login-error">{error}</div>}
    </div>
  );
}
