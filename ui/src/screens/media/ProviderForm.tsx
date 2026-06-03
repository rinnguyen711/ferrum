import type { MediaProviderDescriptor } from "../../api/types";

const MASK = "••••";

export function ProviderForm({
  descriptor, values, onChange, fieldErrors,
}: {
  descriptor: MediaProviderDescriptor;
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
  fieldErrors: Record<string, string>;
}) {
  return (
    <div className="rs-fields">
      {descriptor.fields.map((f) => {
        const err = fieldErrors[f.name];
        const isSecret = f.secret;
        const shown = isSecret ? (values[f.name] === MASK ? "" : values[f.name] ?? "") : (values[f.name] ?? "");
        return (
          <div className="rs-field" key={f.name}>
            <div className="rs-field-label">
              <label>{f.label}{f.required ? " *" : ""}</label>
              {isSecret && <span className="rs-field-hint">Leave blank to keep current</span>}
            </div>
            <input
              className="rs-input"
              type={isSecret ? "password" : "text"}
              value={shown}
              placeholder={isSecret ? "•••• (leave blank to keep)" : ""}
              autoComplete={isSecret ? "new-password" : "off"}
              onChange={(e) => onChange(f.name, e.target.value)}
            />
            {err && <span className="rs-field-error">{err}</span>}
          </div>
        );
      })}
    </div>
  );
}
