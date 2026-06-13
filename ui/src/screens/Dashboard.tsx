import { Link } from "react-router-dom";
import { useResource } from "../hooks/useResource";
import { getHealth, listContentTypes, listEntries } from "../api/endpoints";
import { listWebhooks } from "../api/webhooks";
import { Icons } from "../components/icons";
import { StatCard } from "../components/StatCard";
import { StatusBadge } from "../components/shell";
import { LoadingState, EmptyState } from "../components/ui";
import { relTime } from "../util";
import type { ContentType, Entry } from "../api/types";
import { draftPublishEnabled } from "../api/types";

/** A recent entry tagged with the type it belongs to, so the row can link
 *  back to the right collection. */
type RecentRow = { entry: Entry; type: ContentType };

export function Dashboard() {
  const { data: types, loading, error, refetch } = useResource(
    () => listContentTypes(),
    [],
  );

  // Pull a recent slice from every collection type, not just `article`.
  // One request per collection; cheap for the handful a CMS usually has.
  const collections = (types ?? []).filter((t) => t.kind === "collection");
  const collectionKey = collections.map((t) => t.name).join(",");
  const entries = useResource(
    () =>
      Promise.all(
        collections.map((t) =>
          listEntries(t.name, { pageSize: 100 })
            .then((r) => r.data.map((entry) => ({ entry, type: t })))
            .catch(() => [] as RecentRow[]),
        ),
      ).then((groups) => groups.flat()),
    [collectionKey],
  );

  const health = useResource(() => getHealth().catch(() => null), []);
  const webhooks = useResource(() => listWebhooks().catch(() => null), []);

  if (loading) return <LoadingState />;
  if (error)
    return (
      <EmptyState>
        {error.message}{" "}
        <button className="rs-link-btn" onClick={refetch}>Retry</button>
      </EmptyState>
    );

  const rows: RecentRow[] = entries.data ?? [];
  // Published/draft derive from `published_at` only on types that actually
  // have draft/publish on; a plain collection has no such distinction.
  const dpRows = rows.filter((r) => draftPublishEnabled(r.type));
  const published = dpRows.filter((r) => r.entry.published_at).length;
  const drafts = dpRows.filter((r) => !r.entry.published_at).length;
  const totalEntries = rows.length;

  const recent = [...rows]
    .sort((a, b) => +new Date(b.entry.updated_at) - +new Date(a.entry.updated_at))
    .slice(0, 6);

  const titleOf = (e: Entry) =>
    String(e["title"] ?? e["name"] ?? e.id);

  const newTarget = collections[0];
  const activeWebhooks = webhooks.data?.filter((w) => w.enabled).length ?? null;

  return (
    <div className="rs-dash">
      <div className="rs-dash-hero">
        <div>
          <p className="rs-dash-eyebrow rs-mono">Rustapi · workspace</p>
          <h1>Welcome back</h1>
          <p className="rs-dash-sub">
            {types?.length ?? 0} content types registered. The API is{" "}
            {health.data ? "healthy" : "unreachable"}.
          </p>
        </div>
        {newTarget && (
          <Link to={`/content/${newTarget.name}/new`} className="rs-btn rs-btn--primary">
            <Icons.plus size={17} /> New {newTarget.display_name.toLowerCase()}
          </Link>
        )}
      </div>

      <div className="rs-stat-grid">
        <StatCard label="Published" value={published} delta="live entries" icon="eye" tone="ok" />
        <StatCard label="Drafts" value={drafts} delta="in progress" icon="edit" tone="muted" />
        <StatCard label="Entries" value={totalEntries} delta="across all types" icon="doc" tone="accent" />
        <StatCard
          label="API"
          value={health.data ? `${health.data.db_ms}ms` : "—"}
          delta={health.data ? `v${health.data.version}` : "offline"}
          icon="bolt"
          tone="accent"
          mono
        />
      </div>

      <div className="rs-dash-cols">
        <section className="rs-dash-card">
          <div className="rs-dash-card-head">
            <h2>Recently edited</h2>
            <Link className="rs-link-btn" to="/content">Open Content Manager →</Link>
          </div>
          <div className="rs-dash-list">
            {recent.length === 0 && (
              <div className="rs-empty">
                No entries yet.{" "}
                {newTarget && (
                  <Link className="rs-link-btn" to={`/content/${newTarget.name}/new`}>
                    Create your first entry
                  </Link>
                )}
              </div>
            )}
            {recent.map(({ entry, type }) => (
              <Link className="rs-dash-row" key={`${type.name}:${entry.id}`} to={`/content/${type.name}/${entry.id}`}>
                <span className="rs-dash-row-title">{titleOf(entry)}</span>
                {draftPublishEnabled(type) && (
                  <StatusBadge status={entry.published_at ? "published" : "draft"} />
                )}
                <span className="rs-cell-muted">{relTime(entry.updated_at)}</span>
              </Link>
            ))}
          </div>
        </section>

        <section className="rs-dash-card">
          <div className="rs-dash-card-head">
            <h2>System</h2>
          </div>
          <div className="rs-sys">
            <SysRow
              label="API service"
              value={health.data ? "Healthy" : "Down"}
              sub="axum · in-process"
              ok={!!health.data}
            />
            <SysRow
              label="Database"
              value={health.data ? `${health.data.db_ms}ms` : "Unreachable"}
              sub="PostgreSQL"
              ok={!!health.data}
            />
            <SysRow
              label="Build"
              value={health.data ? `v${health.data.version}` : "—"}
              sub="cargo"
              ok={!!health.data}
              mono
            />
            <SysRow
              label="Webhooks"
              value={activeWebhooks == null ? "—" : `${activeWebhooks} active`}
              sub={activeWebhooks ? "delivering" : "not configured"}
              ok={!!activeWebhooks}
            />
          </div>
        </section>
      </div>
    </div>
  );
}

function SysRow({
  label, value, sub, ok, mono,
}: {
  label: string;
  value: string;
  sub: string;
  ok?: boolean;
  mono?: boolean;
}) {
  return (
    <div className="rs-sys-row">
      <span className={"rs-sys-status" + (ok ? " is-ok" : "")} />
      <div className="rs-sys-meta"><strong>{label}</strong><span className="rs-cell-muted">{sub}</span></div>
      <span className={"rs-sys-val" + (mono ? " rs-mono" : "")}>{value}</span>
    </div>
  );
}
