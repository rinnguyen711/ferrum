import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { useResource } from "../hooks/useResource";
import { getContentType, listContentTypes, listEntries } from "../api/endpoints";
import type { ContentType, Entry, Field } from "../api/types";
import { relationMeta } from "../api/types";
import { relTime, relationLabel, shortId } from "../util";

export function ContentList() {
  const { type = "" } = useParams<{ type: string }>();
  const navigate = useNavigate();

  const schema = useResource(() => getContentType(type), [type]);
  const allTypes = useResource(() => listContentTypes(), []);

  const ct = schema.data;
  const populate = ct
    ? ct.fields.filter((f) => f.kind === "relation").map((f) => f.name).join(",")
    : "";

  const entries = useResource(
    () => listEntries(type, { populate: populate || undefined }),
    [type, populate],
  );

  if (schema.loading || entries.loading) {
    return <div className="rs-empty">Loading…</div>;
  }
  if (schema.error) {
    return (
      <div className="rs-empty">
        Couldn’t load type “{type}”.{" "}
        <button className="rs-link-btn" onClick={schema.refetch}>Retry</button>
      </div>
    );
  }
  if (entries.error) {
    return (
      <div className="rs-empty">
        {entries.error.message}{" "}
        <button className="rs-link-btn" onClick={entries.refetch}>Retry</button>
      </div>
    );
  }
  if (!ct || !entries.data) return <div className="rs-empty">Unknown content type.</div>;

  const rows = entries.data.data;
  const meta = entries.data.meta;
  const cols = ct.fields;

  const targetSchema = (f: Field): ContentType | undefined => {
    const m = relationMeta(f);
    return m ? allTypes.data?.find((t) => t.name === m.target) : undefined;
  };

  const renderCell = (entry: Entry, f: Field): React.ReactNode => {
    const v = entry[f.name];
    if (v == null) return <span className="rs-cell-muted">—</span>;
    switch (f.kind) {
      case "relation":
        return relationLabel(v, targetSchema(f));
      case "datetime":
        return relTime(typeof v === "string" ? v : null);
      case "boolean":
        return v ? <Icons.check size={14} /> : <span className="rs-cell-muted">—</span>;
      case "json":
        return <code className="rs-mono">{JSON.stringify(v)}</code>;
      default:
        return String(v);
    }
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>{ct.display_name}</h1>
          <p className="rs-cm-sub">{meta.total} entries</p>
        </div>
        <button
          className="rs-btn rs-btn--primary"
          onClick={() => navigate(`/content/${type}/new`)}
        >
          <Icons.plus size={16} /> Create new entry
        </button>
      </div>

      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th className="rs-col-id">ID</th>
              {cols.map((f) => (
                <th key={f.name}>{f.name}</th>
              ))}
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((e) => (
              <tr key={e.id} onClick={() => navigate(`/content/${type}/${e.id}`)}>
                <td className="rs-col-id rs-mono">{shortId(e.id)}</td>
                {cols.map((f) => (
                  <td key={f.name}>{renderCell(e, f)}</td>
                ))}
                <td className="rs-cell-muted">{relTime(e.updated_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 0 && <div className="rs-empty">No entries yet.</div>}
      </div>

      <div className="rs-pager">
        <span className="rs-cell-muted">
          Showing {rows.length} of {meta.total}
        </span>
      </div>
    </div>
  );
}
