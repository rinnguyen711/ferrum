import type { ReactNode } from "react";
import { Link, NavLink } from "react-router-dom";
import { Icons, type IconKey } from "./icons";
import { RUSTAPI, type Status } from "../mock/data";
import type { Section } from "../Layout";

export function Avatar({
  name,
  initials,
  color,
  size = 26,
}: {
  name: string;
  initials: string;
  color: string;
  size?: number;
}) {
  return (
    <span
      className="rs-avatar"
      title={name}
      style={{ width: size, height: size, background: color, fontSize: size * 0.4 }}
    >
      {initials}
    </span>
  );
}

const STATUS_MAP: Record<Status, { label: string; cls: string }> = {
  published: { label: "Published", cls: "ok" },
  draft: { label: "Draft", cls: "muted" },
  review: { label: "In review", cls: "warn" },
};

export const STATUS = STATUS_MAP;

export function StatusBadge({ status }: { status: Status }) {
  const s = STATUS_MAP[status] || STATUS_MAP.draft;
  return (
    <span className={"rs-status rs-status--" + s.cls}>
      <span className="rs-dot" />
      {s.label}
    </span>
  );
}

function RailLogo() {
  return (
    <div className="rs-logo" title="Rustapi">
      <svg width={22} height={22} viewBox="0 0 24 24" fill="none">
        <path
          d="M12 2.2 21 7v10l-9 4.8L3 17V7l9-4.8Z"
          stroke="currentColor"
          strokeWidth={1.6}
          strokeLinejoin="round"
        />
        <path
          d="M9 16V8.2h3.6c1.6 0 2.6.9 2.6 2.3 0 1.1-.6 1.9-1.6 2.2L15.4 16h-1.9l-1.5-2.9H10.7V16H9Zm1.7-4.2h1.7c.8 0 1.3-.4 1.3-1.1 0-.7-.5-1.1-1.3-1.1H10.7v2.2Z"
          fill="currentColor"
        />
      </svg>
    </div>
  );
}

export function Sidebar({ section: _section }: { section: Section }) {
  const items: { to: string; label: string; icon: IconKey; end?: boolean }[] = [
    { to: "/", label: "Home", icon: "home", end: true },
    { to: "/content/article", label: "Content Manager", icon: "doc" },
    { to: "/builder/article", label: "Content-Type Builder", icon: "layers" },
    { to: "/media", label: "Media Library", icon: "image" },
  ];
  return (
    <nav className="rs-rail">
      <RailLogo />
      <div className="rs-rail-items">
        {items.map((it) => {
          const I = Icons[it.icon];
          return (
            <NavLink
              key={it.to}
              to={it.to}
              end={it.end}
              data-tip={it.label}
              className={({ isActive }) => "rs-rail-btn" + (isActive ? " is-active" : "")}
            >
              <I size={20} />
            </NavLink>
          );
        })}
      </div>
      <div className="rs-rail-foot">
        <NavLink
          to="/settings"
          data-tip="Settings"
          className={({ isActive }) => "rs-rail-btn" + (isActive ? " is-active" : "")}
        >
          <Icons.gear size={20} />
        </NavLink>
        <Avatar name="Mara Velez" initials="MV" color="#C2410C" size={30} />
      </div>
    </nav>
  );
}

function PanelGroup({
  label,
  count: _count,
  action,
  children,
}: {
  label: string;
  count: number;
  action?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="rs-panel-group">
      <div className="rs-panel-grouphead">
        <span>{label}</span>
        {action && (
          <button className="rs-panel-add" title={"New " + label.toLowerCase()}>
            <Icons.plus size={14} />
          </button>
        )}
      </div>
      {children}
    </div>
  );
}

export function SecondaryPanel({
  section,
  collection,
}: {
  section: Section;
  collection: string;
}) {
  if (section === "dashboard" || section === "media") return null;

  if (section === "content" || section === "builder") {
    const isBuilder = section === "builder";
    const base = isBuilder ? "/builder" : "/content";
    const collTypes = Object.values(RUSTAPI.types);
    const counts: Record<string, number> = {
      article: RUSTAPI.articles.length,
      author: RUSTAPI.authors.length,
      category: RUSTAPI.categories.length,
    };
    return (
      <aside className="rs-panel">
        <div className="rs-panel-head">
          <h2>{isBuilder ? "Content-Type Builder" : "Content Manager"}</h2>
        </div>
        <div className="rs-panel-scroll">
          <div className="rs-panel-search">
            <Icons.search size={15} />
            <input placeholder="Search types" />
          </div>
          <PanelGroup label="Collection types" count={collTypes.length} action={isBuilder}>
            {collTypes.map((t) => (
              <Link
                key={t.key}
                to={`${base}/${t.key}`}
                className={"rs-panel-item" + (collection === t.key ? " is-active" : "")}
              >
                {t.plural}
                <span className="rs-panel-count">{counts[t.key] ?? 0}</span>
              </Link>
            ))}
          </PanelGroup>
          <PanelGroup label="Single types" count={RUSTAPI.singleTypes.length} action={isBuilder}>
            {RUSTAPI.singleTypes.map((t) => (
              <button key={t.key} className="rs-panel-item">
                {t.display}
              </button>
            ))}
          </PanelGroup>
          {isBuilder && (
            <PanelGroup label="Components" count={2} action>
              <button className="rs-panel-item">SEO</button>
              <button className="rs-panel-item">Call to action</button>
            </PanelGroup>
          )}
        </div>
      </aside>
    );
  }

  if (section === "settings") {
    const groups = [
      { label: "Global settings", items: ["Overview", "API tokens", "Webhooks", "Internationalization"] },
      { label: "Administration", items: ["Users", "Roles", "Audit logs", "Single sign-on"] },
    ];
    return (
      <aside className="rs-panel">
        <div className="rs-panel-head">
          <h2>Settings</h2>
        </div>
        <div className="rs-panel-scroll">
          {groups.map((g) => (
            <div className="rs-panel-group" key={g.label}>
              <div className="rs-panel-grouphead">
                <span>{g.label}</span>
              </div>
              {g.items.map((it) => (
                <button
                  key={it}
                  className={
                    "rs-panel-item" +
                    (g.label === "Global settings" && it === "API tokens" ? " is-active" : "")
                  }
                >
                  {it}
                </button>
              ))}
            </div>
          ))}
        </div>
      </aside>
    );
  }
  return null;
}

export function HealthPill() {
  return (
    <div className="rs-health" data-tip="Rust API · axum 0.7 · all systems healthy">
      <span className="rs-health-dot" />
      <span className="rs-health-text">API healthy</span>
      <span className="rs-health-sep" />
      <span className="rs-mono rs-health-lat">11ms p99</span>
    </div>
  );
}

export function Topbar({
  title,
  crumbs,
  right,
  dark,
  onToggleDark,
}: {
  title?: string;
  crumbs?: string[];
  right?: ReactNode;
  dark: boolean;
  onToggleDark: () => void;
}) {
  return (
    <header className="rs-topbar">
      <div className="rs-crumbs">
        {crumbs &&
          crumbs.map((c, i) => (
            <span key={i} className="rs-crumb-wrap" style={{ display: "contents" }}>
              {i > 0 && <Icons.chevRight size={14} className="rs-crumb-sep" />}
              <span className={"rs-crumb" + (i === crumbs.length - 1 ? " is-last" : "")}>{c}</span>
            </span>
          ))}
        {!crumbs && <span className="rs-crumb is-last">{title}</span>}
      </div>
      <div className="rs-topbar-right">
        {right}
        <HealthPill />
        <button
          className="rs-icon-btn"
          data-tip={dark ? "Light mode" : "Dark mode"}
          onClick={onToggleDark}
          aria-label="Toggle dark mode"
        >
          {dark ? <Icons.sun size={18} /> : <Icons.moon size={18} />}
        </button>
        <button className="rs-icon-btn" data-tip="Notifications">
          <Icons.bell size={18} />
          <span className="rs-bell-dot" />
        </button>
        <div className="rs-topbar-user">
          <Avatar name="Mara Velez" initials="MV" color="#C2410C" size={28} />
          <div className="rs-topbar-user-meta">
            <strong>Mara Velez</strong>
            <span>Editor in chief</span>
          </div>
          <Icons.chevDown size={15} />
        </div>
      </div>
    </header>
  );
}
