import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useResource } from "../hooks/useResource";
import { deleteContentType, getContentType } from "../api/endpoints";
import { relationMeta, enumValues } from "../api/types";
import type { Field } from "../api/types";
import { ApiError } from "../api/client";

export function ContentTypeBuilder() {
  const { type = "" } = useParams<{ type: string }>();
  const { data: ct, loading, error, refetch } = useResource(
    () => getContentType(type),
    [type],
  );

  const navigate = useNavigate();
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);

  const doDelete = async () => {
    setDeleting(true);
    setBanner(null);
    try {
      await deleteContentType(type);
      navigate("/builder");
    } catch (e) {
      setBanner(e instanceof ApiError ? e.message : "Delete failed.");
      setConfirming(false);
    } finally {
      setDeleting(false);
    }
  };

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
        {confirming ? (
          <div className="rs-confirm">
            <span>Delete <strong>{ct.name}</strong>? Drops the type and all its entries.</span>
            <button
              className="rs-btn rs-btn--ghost rs-btn--sm rs-danger"
              onClick={doDelete}
              disabled={deleting}
            >
              {deleting ? "Deleting…" : "Confirm"}
            </button>
            <button
              className="rs-btn rs-btn--ghost rs-btn--sm"
              onClick={() => setConfirming(false)}
              disabled={deleting}
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            className="rs-btn rs-btn--ghost rs-danger"
            onClick={() => setConfirming(true)}
          >
            Delete type
          </button>
        )}
      </div>

      {banner && <div className="rs-login-error" style={{ marginBottom: 12 }}>{banner}</div>}
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
