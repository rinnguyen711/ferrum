import { Icons } from "../components/icons";
import type { DraftField } from "./draftModel";
import type { FieldKind } from "../api/types";

// kind → icon key in ui/src/components/icons.tsx.
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
  media: "image",
  enum: "layers",
  json: "braces",
  email: "mail",
  url: "link",
  rich_text: "doc",
  component: "layers",
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
  index,
  count,
  reorderable,
  dragOver,
  onEdit,
  onRemove,
  onMove,
  onDragStart,
  onDragEnter,
  onDragEnd,
  onDrop,
}: {
  field: DraftField;
  index: number;
  count: number;
  reorderable: boolean;
  dragOver?: boolean;
  onEdit: () => void;
  onRemove: () => void;
  onMove: (dir: -1 | 1) => void;
  onDragStart: () => void;
  onDragEnter: () => void;
  onDragEnd: () => void;
  onDrop: () => void;
}) {
  const I = Icons[KIND_ICON[field.kind] ?? "type"];
  const meta = metaText(field);
  const label = field.name || "untitled";
  return (
    <div
      className={"rs-schema-row" + (dragOver ? " is-drag-over" : "")}
      role="row"
      draggable={reorderable}
      onDragStart={reorderable ? onDragStart : undefined}
      onDragEnter={reorderable ? onDragEnter : undefined}
      onDragOver={reorderable ? (e) => e.preventDefault() : undefined}
      onDragEnd={reorderable ? onDragEnd : undefined}
      onDrop={reorderable ? (e) => { e.preventDefault(); onDrop(); } : undefined}
    >
      {reorderable ? (
        <button
          className="rs-schema-drag"
          role="cell"
          aria-label={`Reorder ${label}, position ${index + 1} of ${count}. Use arrow up and down keys to move.`}
          onKeyDown={(e) => {
            if (e.key === "ArrowUp") { e.preventDefault(); onMove(-1); }
            else if (e.key === "ArrowDown") { e.preventDefault(); onMove(1); }
          }}
        >
          <Icons.drag size={16} aria-hidden="true" />
        </button>
      ) : (
        <span className="rs-schema-drag" role="cell" />
      )}
      <div className="rs-schema-fieldicon" role="cell"><I size={16} aria-hidden="true" /></div>
      <div className="rs-schema-name" role="cell">
        <strong className="rs-mono">{label}</strong>
        {field.required && <span className="rs-req-tag">required</span>}
      </div>
      <div className="rs-schema-type" role="cell">
        <span className="rs-type-pill">{field.kind}</span>
        {meta && <span className="rs-cell-muted">{meta}</span>}
      </div>
      <div className="rs-schema-actions" role="cell">
        <button className="rs-row-btn" onClick={onEdit} aria-label={`Edit ${label}`}>
          <Icons.edit size={15} aria-hidden="true" />
        </button>
        <button className="rs-row-btn rs-danger" onClick={onRemove} aria-label={`Remove ${label}`}>
          <Icons.trash size={15} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
