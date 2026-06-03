import { useEffect, useState, type ReactNode } from "react";
import { useLocation } from "react-router-dom";
import { Icons, type IconKey } from "./icons";
import { getHealth, listContentTypes } from "../api/endpoints";
import type { Health, PatchContentType } from "../api/types";
import { useResource } from "../hooks/useResource";
import type { Section } from "../Layout";
import { useBuilderDraft } from "../builder/BuilderDraftContext";
import { diffToPatch } from "../builder/draftModel";
import { CreateTypeModal } from "../builder/CreateTypeModal";
import { SaveConfirmModal } from "../builder/SaveConfirmModal";

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

type Status = "published" | "draft" | "review";

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
  const location = useLocation();
  const builder = useBuilderDraft();
  const items: { to: string; label: string; icon: IconKey; end?: boolean }[] = [
    { to: "/", label: "Home", icon: "home", end: true },
    { to: "/content", label: "Content Manager", icon: "doc" },
    { to: "/builder", label: "Content-Type Builder", icon: "layers" },
    { to: "/media", label: "Media Library", icon: "image" },
  ];
  const isActive = (to: string, end?: boolean) =>
    end ? location.pathname === to : location.pathname.startsWith(to);
  return (
    <nav className="rs-rail">
      <RailLogo />
      <div className="rs-rail-items">
        {items.map((it) => {
          const I = Icons[it.icon];
          return (
            <button
              key={it.to}
              data-tip={it.label}
              className={"rs-rail-btn" + (isActive(it.to, it.end) ? " is-active" : "")}
              onClick={() => builder.guardedNavigate(it.to)}
            >
              <I size={20} />
            </button>
          );
        })}
      </div>
      <div className="rs-rail-foot">
        <button
          data-tip="Settings"
          className={"rs-rail-btn" + (location.pathname.startsWith("/settings") ? " is-active" : "")}
          onClick={() => builder.guardedNavigate("/settings")}
        >
          <Icons.gear size={20} />
        </button>
        <Avatar name="Admin" initials="AD" color="#52525B" size={30} />
      </div>
    </nav>
  );
}

function PanelGroup({
  label,
  count: _count,
  action,
  onAction,
  children,
}: {
  label: string;
  count: number;
  action?: boolean;
  onAction?: () => void;
  children: ReactNode;
}) {
  return (
    <div className="rs-panel-group">
      <div className="rs-panel-grouphead">
        <span>{label}</span>
        {action && (
          <button
            className="rs-panel-add"
            title={"New " + label.toLowerCase()}
            onClick={onAction}
          >
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
    return <TypePanel base={base} isBuilder={isBuilder} collection={collection} />;
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
              {g.items.map((it) => {
                const enabled = g.label === "Global settings" && it === "API tokens";
                return (
                  <button
                    key={it}
                    disabled={!enabled}
                    title={enabled ? undefined : "Coming soon"}
                    className={"rs-panel-item" + (enabled ? " is-active" : "")}
                  >
                    {it}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      </aside>
    );
  }
  return null;
}

function TypePanel({
  base,
  isBuilder,
  collection,
}: {
  base: string;
  isBuilder: boolean;
  collection: string;
}) {
  const location = useLocation();
  const [modalOpen, setModalOpen] = useState(false);
  const [confirmPatch, setConfirmPatch] = useState<PatchContentType | null>(null);
  const builder = useBuilderDraft();
  // Refetch only when entering this section (base) or after a save — not on
  // every collection click, which just changes the path suffix.
  const { data: types, loading, error } = useResource(
    () => listContentTypes(),
    [base, builder.saveNonce],
  );

  const onSaveClick = () => {
    const d = builder.draft;
    if (!d) return;
    if (d.mode === "existing") {
      const patch = diffToPatch(d);
      if (patch.drop_fields.length > 0) {
        setConfirmPatch(patch);
        return;
      }
    }
    void builder.save();
  };

  return (
    <aside className="rs-panel">
      <div className="rs-panel-head">
        <h2>{isBuilder ? "Content-Type Builder" : "Content Manager"}</h2>
        {isBuilder && (
          <button
            className={"rs-btn rs-btn--sm " + (builder.dirty ? "rs-btn--primary" : "rs-btn--ghost")}
            disabled={!builder.dirty || builder.saving}
            onClick={onSaveClick}
            title="Save schema changes"
          >
            {builder.saving ? "Saving…" : "Save"}
          </button>
        )}
      </div>
      <div className="rs-panel-scroll">
        <div className="rs-panel-search">
          <Icons.search size={15} />
          <input placeholder="Search types" disabled />
        </div>
        <PanelGroup
          label="Collection types"
          count={types?.length ?? 0}
          action={isBuilder}
          onAction={() => {
            if (builder.dirty) {
              if (!window.confirm("You have unsaved changes. Discard them and create a new type?")) return;
              builder.reset();
            }
            setModalOpen(true);
          }}
        >
          {loading && !types &&
            [72, 56, 64, 48].map((w, i) => (
              <div key={i} className="rs-skel" style={{ width: `${w}%` }} />
            ))}
          {error && !types && <div className="rs-panel-item rs-danger">Failed to load</div>}
          {types?.map((t) => (
            <button
              key={t.name}
              onClick={() => builder.guardedNavigate(`${base}/${t.name}`)}
              className={"rs-panel-item rs-panel-item--btn" + (collection === t.name ? " is-active" : "")}
            >
              {t.display_name}
            </button>
          ))}
        </PanelGroup>
        <div className="rs-panel-group">
          <div className="rs-panel-grouphead"><span>Single types</span></div>
          <button className="rs-panel-item" disabled title="Coming soon">Homepage</button>
          <button className="rs-panel-item" disabled title="Coming soon">Global</button>
        </div>
        {isBuilder && (
          <div className="rs-panel-group">
            <div className="rs-panel-grouphead"><span>Components</span></div>
            <button className="rs-panel-item" disabled title="Coming soon">SEO</button>
            <button className="rs-panel-item" disabled title="Coming soon">Call to action</button>
          </div>
        )}
      </div>
      {modalOpen && <CreateTypeModal onClose={() => setModalOpen(false)} />}
      {confirmPatch && (
        <SaveConfirmModal
          patch={confirmPatch}
          saving={builder.saving}
          onConfirm={() => { setConfirmPatch(null); void builder.save(); }}
          onCancel={() => setConfirmPatch(null)}
        />
      )}
    </aside>
  );
}

export function HealthPill() {
  const [health, setHealth] = useState<Health | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let ignore = false;
    getHealth()
      .then((h) => {
        if (!ignore) setHealth(h);
      })
      .catch(() => {
        if (!ignore) setFailed(true);
      });
    return () => {
      ignore = true;
    };
  }, []);

  if (failed) {
    return (
      <div className="rs-health rs-health--down" data-tip="API unreachable">
        <span className="rs-health-dot" />
        <span className="rs-health-text">API unreachable</span>
      </div>
    );
  }
  if (!health) {
    return (
      <div className="rs-health" data-tip="Checking…">
        <span className="rs-health-dot" />
        <span className="rs-health-text">Checking…</span>
      </div>
    );
  }
  return (
    <div className="rs-health" data-tip={`Rust API · v${health.version}`}>
      <span className="rs-health-dot" />
      <span className="rs-health-text">API healthy</span>
      <span className="rs-health-sep" />
      <span className="rs-mono rs-health-lat">v{health.version} · {health.db_ms}ms</span>
    </div>
  );
}

export function Topbar({
  title,
  crumbs,
  right,
  dark,
  onToggleDark,
  email,
  onLogout,
}: {
  title?: string;
  crumbs?: string[];
  right?: ReactNode;
  dark: boolean;
  onToggleDark: () => void;
  email?: string | null;
  onLogout?: () => void;
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
          <Avatar
            name={email ?? "Admin"}
            initials={(email ?? "AD").slice(0, 2).toUpperCase()}
            color="#52525B"
            size={28}
          />
          <div className="rs-topbar-user-meta">
            <strong>{email ?? "Admin"}</strong>
            <span>Signed in</span>
          </div>
          <button
            className="rs-icon-btn"
            data-tip="Sign out"
            onClick={onLogout}
            aria-label="Sign out"
          >
            <Icons.arrowLeft size={18} />
          </button>
        </div>
      </div>
    </header>
  );
}
