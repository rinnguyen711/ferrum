import { Icons } from "../components/icons";
import type { DraftField } from "./draftModel";
import type { FieldKind } from "../api/types";

// kind → icon key in ui/src/components/icons.tsx. icons.tsx has no
// braces/mail keys, so json/email fall back to doc/type.
const KIND_ICON: Record<FieldKind, keyof typeof Icons> = {
  string: "type",
  text: "doc",
  slug: "hash",
  integer: "hash",
  float: "hash",
  boolean: "toggle",
  datetime: "calendar",
  uuid: "hash",
  relation: "relation",
  enum: "layers",
  json: "doc",
  email: "type",
  url: "link",
};

function metaText(f: DraftField): string {
  switch (f.kind) {
    case "relation": {
      const arrow =
        f.cardinality === "many_to_many" ? "↔ many" :
        f.cardinality === "one_to_one" ? "↔" : "→";
      return `${arrow} ${f.target || "—"}`;
    }
    case "enum":
      return f.enumValues.length ? `${f.enumValues.length} values` : "enumeration";
    default:
      return "";
  }
}

export function FieldRow({
  field,
  onEdit,
  onRemove,
}: {
  field: DraftField;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const I = Icons[KIND_ICON[field.kind] ?? "type"];
  const meta = metaText(field);
  return (
    <div className="rs-schema-row">
      <span className="rs-schema-drag"><Icons.drag size={16} /></span>
      <div className="rs-schema-fieldicon"><I size={16} /></div>
      <div className="rs-schema-name">
        <strong className="rs-mono">{field.name || "untitled"}</strong>
        {field.required && <span className="rs-req-tag">required</span>}
      </div>
      <div className="rs-schema-type">
        <span className="rs-type-pill">{field.kind}</span>
        {meta && <span className="rs-cell-muted">{meta}</span>}
      </div>
      <div className="rs-schema-actions">
        <button className="rs-row-btn" onClick={onEdit} title="Edit field">
          <Icons.edit size={15} />
        </button>
        <button className="rs-row-btn rs-danger" onClick={onRemove} title="Remove field">
          <Icons.trash size={15} />
        </button>
      </div>
    </div>
  );
}
