import { useParams } from "react-router-dom";
import { useResource } from "../hooks/useResource";
import { getContentType } from "../api/endpoints";
import { relationMeta, enumValues } from "../api/types";
import type { Field } from "../api/types";

export function ContentTypeBuilder() {
  const { type = "" } = useParams<{ type: string }>();
  const { data: ct, loading, error, refetch } = useResource(
    () => getContentType(type),
    [type],
  );

  if (loading) return <div className="rs-empty">Loading…</div>;
  if (error)
    return (
      <div className="rs-empty">
        {error.message}{" "}
        <button className="rs-link-btn" onClick={refetch}>
          Retry
        </button>
      </div>
    );
  if (!ct) return <div className="rs-empty">Unknown content type.</div>;

  const meta = (f: Field): string => {
    if (f.kind === "relation") {
      const m = relationMeta(f);
      return m ? `→ ${m.target} (${m.cardinality})` : "relation";
    }
    if (f.kind === "enum") return enumValues(f).join(" · ");
    return "";
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>{ct.display_name}</h1>
          <p className="rs-cm-sub rs-mono">{ct.name} · {ct.fields.length} fields (read-only)</p>
        </div>
      </div>
      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th>Field</th>
              <th>Kind</th>
              <th>Required</th>
              <th>Unique</th>
              <th>Meta</th>
            </tr>
          </thead>
          <tbody>
            {ct.fields.map((f) => (
              <tr key={f.name}>
                <td className="rs-cell-title">{f.name}</td>
                <td className="rs-mono">{f.kind}</td>
                <td>{f.required ? "yes" : "—"}</td>
                <td>{f.unique ? "yes" : "—"}</td>
                <td className="rs-cell-muted">{meta(f)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
