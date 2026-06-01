import { useParams } from "react-router-dom";
import { Icons, type IconKey } from "../components/icons";
import { RUSTAPI } from "../mock/data";

const TYPE_ICON: Record<string, IconKey> = {
  Text: "type",
  UID: "hash",
  Enumeration: "layers",
  Media: "image",
  "Rich text": "doc",
  Relation: "relation",
  Boolean: "toggle",
  Number: "hash",
  Datetime: "calendar",
};

export function ContentTypeBuilder() {
  const { type = "article" } = useParams<{ type: string }>();
  const t = RUSTAPI.types[type] ?? RUSTAPI.types.article;
  return (
    <div className="rs-builder">
      <div className="rs-cm-head">
        <div>
          <h1>{t.display}</h1>
          <p className="rs-cm-sub rs-mono">
            api::{t.key}.{t.key} · {t.fields.length} fields · collection type
          </p>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost">
            <Icons.eye size={15} /> Preview API
          </button>
          <button className="rs-btn rs-btn--primary">
            <Icons.plus size={16} /> Add another field
          </button>
        </div>
      </div>
      <div className="rs-schema">
        <div className="rs-schema-head">
          <span>Field</span>
          <span>Type</span>
          <span />
        </div>
        {t.fields.map((f) => {
          const I = Icons[TYPE_ICON[f.type] ?? "type"];
          return (
            <div className="rs-schema-row" key={f.name}>
              <span className="rs-schema-drag">
                <Icons.drag size={16} />
              </span>
              <div className="rs-schema-fieldicon">
                <I size={16} />
              </div>
              <div className="rs-schema-name">
                <strong className="rs-mono">{f.name}</strong>
                {f.required && <span className="rs-req-tag">required</span>}
              </div>
              <div className="rs-schema-type">
                <span className="rs-type-pill">{f.type}</span>
                {f.meta && <span className="rs-cell-muted">{f.meta}</span>}
              </div>
              <div className="rs-schema-actions">
                <button className="rs-row-btn">
                  <Icons.edit size={15} />
                </button>
                <button className="rs-row-btn rs-danger">
                  <Icons.trash size={15} />
                </button>
              </div>
            </div>
          );
        })}
        <button className="rs-schema-add">
          <Icons.plus size={16} /> Add another field to this collection type
        </button>
      </div>
    </div>
  );
}
