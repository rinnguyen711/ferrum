import { Link } from "react-router-dom";
import { useResource } from "../hooks/useResource";
import { getHealth, listContentTypes, listEntries } from "../api/endpoints";
import { Icons } from "../components/icons";
import { StatusBadge } from "../components/shell";
import { LoadingState, EmptyState } from "../components/ui";
import { relTime } from "../util";
import type { Entry } from "../api/types";

export function Dashboard() {
  const { data: types, loading, error, refetch } = useResource(
    () => listContentTypes(),
    [],
  );
  const hasArticle = !!types?.some((t) => t.name === "article");
  const articles = useResource(
    () => (hasArticle ? listEntries("article", { pageSize: 100 }) : Promise.resolve(null)),
    [hasArticle],
  );
  const health = useResource(() => getHealth().catch(() => null), []);

  if (loading) return <LoadingState />;
  if (error)
    return (
      <EmptyState>
        {error.message}{" "}
        <button className="rs-link-btn" onClick={refetch}>Retry</button>
      </EmptyState>
    );

  const rows = (articles.data?.data ?? []) as Entry[];
  const byStatus = (s: string) => rows.filter((a) => {
    if (s === "published") return !!a.published_at;
    if (s === "draft") return !a.published_at;
    return false;
  }).length;
  const recent = [...rows]
    .sort((a, b) => +new Date(b.updated_at) - +new Date(a.updated_at))
    .slice(0, 5);

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
        {hasArticle && (
          <Link to="/content/article/new" className="rs-btn rs-btn--primary">
            <Icons.plus size={17} /> New article
          </Link>
        )}
      </div>

      <div className="rs-stat-grid">
        <StatCard label="Published" value={byStatus("published")} delta="live entries" icon="eye" tone="ok" />
        <StatCard label="In review" value={byStatus("review")} delta="needs attention" icon="clock" tone="warn" />
        <StatCard label="Drafts" value={byStatus("draft")} delta="in progress" icon="edit" tone="muted" />
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
            {recent.length === 0 && <div className="rs-empty">No recent entries.</div>}
            {recent.map((a) => (
              <Link className="rs-dash-row" key={a.id} to={`/content/article/${a.id}`}>
                <span className="rs-dash-row-title">{String(a["title"] ?? a.id)}</span>
                <StatusBadge status={a.published_at ? "published" : "draft"} />
                <span className="rs-cell-muted">{relTime(a.updated_at)}</span>
              </Link>
            ))}
          </div>
        </section>

        <section className="rs-dash-card">
          <div className="rs-dash-card-head">
            <h2>System</h2>
            <span className="rs-preview-pill">preview</span>
          </div>
          <div className="rs-sys">
            <SysRow label="API service" value={health.data ? "Healthy" : "Down"} sub="axum · in-process" ok={!!health.data} />
            <SysRow label="Database" value="Healthy" sub="PostgreSQL 16" ok />
            <SysRow label="Build" value={health.data ? `v${health.data.version}` : "—"} sub="cargo" mono />
            <SysRow label="Webhooks" value="0 active" sub="not configured" />
          </div>
          <div className="rs-spark">
            <div className="rs-spark-head"><span>Requests · last hour</span><strong className="rs-mono">—</strong></div>
            <svg viewBox="0 0 240 48" preserveAspectRatio="none" className="rs-spark-svg">
              <polyline
                points="0,40 20,34 40,36 60,28 80,30 100,20 120,24 140,14 160,18 180,10 200,16 220,8 240,12"
                fill="none" stroke="var(--accent)" strokeWidth={2}
              />
            </svg>
          </div>
        </section>
      </div>
    </div>
  );
}

function StatCard({
  label, value, delta, icon, tone, mono,
}: {
  label: string;
  value: string | number;
  delta: string;
  icon: "eye" | "clock" | "edit" | "bolt";
  tone: string;
  mono?: boolean;
}) {
  const I = Icons[icon];
  return (
    <div className={"rs-stat rs-stat--" + tone}>
      <div className="rs-stat-icon"><I size={18} /></div>
      <div className="rs-stat-body">
        <span className="rs-stat-label">{label}</span>
        <strong className={"rs-stat-value" + (mono ? " rs-mono" : "")}>{value}</strong>
        <span className="rs-stat-delta">{delta}</span>
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
