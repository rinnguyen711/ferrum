import { useNavigate } from "react-router-dom";
import { Avatar, StatusBadge } from "../components/shell";
import { Icons, type IconKey } from "../components/icons";
import { RUSTAPI, relTime } from "../mock/data";

function StatCard({
  label,
  value,
  delta,
  icon,
  tone,
  mono,
}: {
  label: string;
  value: string | number;
  delta: string;
  icon: IconKey;
  tone: "ok" | "warn" | "muted" | "accent";
  mono?: boolean;
}) {
  const I = Icons[icon];
  return (
    <div className={"rs-stat rs-stat--" + tone}>
      <div className="rs-stat-icon">
        <I size={18} />
      </div>
      <div className="rs-stat-body">
        <span className="rs-stat-label">{label}</span>
        <strong className={"rs-stat-value" + (mono ? " rs-mono" : "")}>{value}</strong>
        <span className="rs-stat-delta">{delta}</span>
      </div>
    </div>
  );
}

function SysRow({
  label,
  value,
  sub,
  ok,
  mono,
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
      <div className="rs-sys-meta">
        <strong>{label}</strong>
        <span className="rs-cell-muted">{sub}</span>
      </div>
      <span className={"rs-sys-val" + (mono ? " rs-mono" : "")}>{value}</span>
    </div>
  );
}

export function Dashboard() {
  const navigate = useNavigate();
  const openEntry = (id: number | "new") =>
    navigate(`/content/article/${id}`);

  const recent = [...RUSTAPI.articles]
    .sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
    .slice(0, 5);
  const published = RUSTAPI.articles.filter((a) => a.status === "published").length;
  const drafts = RUSTAPI.articles.filter((a) => a.status === "draft").length;
  const review = RUSTAPI.articles.filter((a) => a.status === "review").length;

  return (
    <div className="rs-dash">
      <div className="rs-dash-hero">  
        <div>
          <p className="rs-dash-eyebrow rs-mono">Aurora Journal · workspace</p>
          <h1>Good Morning, Mara</h1>
          <p className="rs-dash-sub">
            3 entries are waiting for your review, and the API has had zero errors in the last 24
            hours.
          </p>
        </div>
        <button
          className="rs-btn rs-btn--primary rs-btn--lg"
          onClick={() => openEntry("new")}
        >
          <Icons.plus size={17} /> New article
        </button>
      </div>

      <div className="rs-stat-grid">
        <StatCard label="Published" value={published} delta="+4 this week" icon="eye" tone="ok" />
        <StatCard label="In review" value={review} delta="needs attention" icon="clock" tone="warn" />
        <StatCard label="Drafts" value={drafts} delta="2 updated today" icon="edit" tone="muted" />
        <StatCard label="p99 latency" value="11ms" delta="0 errors / 24h" icon="bolt" tone="accent" mono />
      </div>

      <div className="rs-dash-cols">
        <section className="rs-dash-card">
          <div className="rs-dash-card-head">
            <h2>Recently edited</h2>
            <button className="rs-link-btn" onClick={() => navigate("/content/article")}>
              Open Content Manager →
            </button>
          </div>
          <div className="rs-dash-list">
            {recent.map((a) => {
              const au = RUSTAPI.authors.find((x) => x.id === a.author)!;
              return (
                <button className="rs-dash-row" key={a.id} onClick={() => openEntry(a.id)}>
                  <Avatar name={au.name} initials={au.avatar} color={au.color} size={28} />
                  <span className="rs-dash-row-title">{a.title}</span>
                  <StatusBadge status={a.status} />
                  <span className="rs-cell-muted">{relTime(a.updatedAt)}</span>
                </button>
              );
            })}
          </div>
        </section>

        <section className="rs-dash-card">
          <div className="rs-dash-card-head">
            <h2>System</h2>
            <span className="rs-health-dot" />
          </div>
          <div className="rs-sys">
            <SysRow label="API service" value="Healthy" sub="axum 0.7 · 3 replicas" ok />
            <SysRow label="Database" value="Healthy" sub="PostgreSQL 16 · 4ms" ok />
            <SysRow label="Build" value="v0.9.2" sub="cargo · 2h ago" mono />
            <SysRow label="Webhooks" value="2 active" sub="last fired 11m ago" />
          </div>
          <div className="rs-spark">
            <div className="rs-spark-head">
              <span>Requests · last hour</span>
              <strong className="rs-mono">18.4k</strong>
            </div>
            <svg viewBox="0 0 240 48" preserveAspectRatio="none" className="rs-spark-svg">
              <polyline
                points="0,40 20,34 40,36 60,28 80,30 100,20 120,24 140,14 160,18 180,10 200,16 220,8 240,12"
                fill="none"
                stroke="var(--accent)"
                strokeWidth={2}
              />
              <polyline
                points="0,40 20,34 40,36 60,28 80,30 100,20 120,24 140,14 160,18 180,10 200,16 220,8 240,12 240,48 0,48"
                fill="var(--accent)"
                opacity={0.08}
                stroke="none"
              />
            </svg>
          </div>
        </section>
      </div>
    </div>
  );
}
