import { Link } from "react-router-dom";
import { useResource } from "../hooks/useResource";
import { listContentTypes } from "../api/endpoints";

export function Dashboard() {
  const { data: types, loading, error, refetch } = useResource(
    () => listContentTypes(),
    [],
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

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Home</h1>
          <p className="rs-cm-sub">{types?.length ?? 0} content types</p>
        </div>
      </div>
      <div className="rs-cards">
        {types?.map((t) => (
          <Link key={t.name} to={`/content/${t.name}`} className="rs-author-card">
            <div className="rs-author-meta">
              <strong>{t.display_name}</strong>
              <span className="rs-cell-muted rs-mono">{t.name}</span>
            </div>
            <p className="rs-author-bio">{t.fields.length} fields</p>
          </Link>
        ))}
      </div>
    </div>
  );
}
