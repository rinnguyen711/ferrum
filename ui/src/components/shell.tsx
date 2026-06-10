import { useEffect, useState, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Icons, type IconKey } from "./icons";
import { getHealth, listContentTypes, listComponents } from "../api/endpoints";
import type { Health, PatchContentType, Component } from "../api/types";
import { useResource } from "../hooks/useResource";
import type { Section } from "../Layout";
import { useBuilderDraft } from "../builder/BuilderDraftContext";
import { getClaims } from "../auth";
import { diffToPatch } from "../builder/draftModel";
import { CreateTypeModal } from "../builder/CreateTypeModal";
import { CreateSingleTypeModal } from "../builder/CreateSingleTypeModal";
import { CreateComponentModal } from "../builder/CreateComponentModal";
import { SaveConfirmModal } from "../builder/SaveConfirmModal";
import { initials, AVATAR_NEUTRAL } from "../util";

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
      <img
        src={`${import.meta.env.BASE_URL}logo.png`}
        alt="Rustapi"
        width={26}
        height={26}
        style={{ display: "block" }}
      />
    </div>
  );
}

export function Sidebar({ section: _section }: { section: Section }) {
  const location = useLocation();
  const builder = useBuilderDraft();
  const isAdmin = (getClaims()?.roles ?? []).includes("admin");
  const items: { to: string; label: string; icon: IconKey; end?: boolean }[] = [
    { to: "/", label: "Home", icon: "home", end: true },
    ...(isAdmin
      ? [{ to: "/users", label: "Users & Permissions", icon: "user" as IconKey }]
      : []),
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
        <Avatar name="Admin" initials="AD" color={AVATAR_NEUTRAL} size={30} />
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

  if (section === "users") {
    return <UsersPanel />;
  }

  if (section === "settings") {
    return <SettingsPanel />;
  }
  return null;
}

/** Secondary panel for Settings. Items with a `to` are live; the rest are
 * placeholders pending their screens. */
function SettingsPanel() {
  const location = useLocation();
  const navigate = useNavigate();

  type Item = { label: string; to?: string };
  const groups: { label: string; items: Item[] }[] = [
    {
      label: "Global settings",
      items: [
        { label: "Overview" },
        { label: "API tokens", to: "/settings/api-tokens" },
        { label: "Media storage", to: "/settings/media" },
        { label: "Webhooks", to: "/settings/webhooks" },
        { label: "Internationalization" },
      ],
    },
    {
      label: "Administration",
      items: [
        { label: "Users" },
        { label: "Roles" },
        { label: "Audit logs" },
        { label: "Single sign-on" },
      ],
    },
  ];

  const isActive = (to?: string) =>
    to === "/settings"
      ? location.pathname === "/settings"
      : !!to && location.pathname === to;

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
              const enabled = !!it.to;
              return (
                <button
                  key={it.label}
                  disabled={!enabled}
                  title={enabled ? undefined : "Coming soon"}
                  className={"rs-panel-item" + (isActive(it.to) ? " is-active" : "")}
                  onClick={enabled ? () => navigate(it.to!) : undefined}
                >
                  {it.label}
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </aside>
  );
}

/** Secondary panel for the Users & Permissions section. Users is live; the
 * rest are placeholders for upcoming slices (roles, audit, SSO). */
function UsersPanel() {
  const location = useLocation();
  const builder = useBuilderDraft();
  const onUsers = location.pathname.startsWith("/users");
  const onRoles = location.pathname.startsWith("/roles");
  return (
    <aside className="rs-panel">
      <div className="rs-panel-head">
        <h2>Users &amp; Permissions</h2>
      </div>
      <div className="rs-panel-scroll">
        <div className="rs-panel-group">
          <div className="rs-panel-grouphead">
            <span>Access</span>
          </div>
          <button
            className={"rs-panel-item" + (onUsers ? " is-active" : "")}
            onClick={() => builder.guardedNavigate("/users")}
          >
            Users
          </button>
          <button
            className={"rs-panel-item" + (onRoles ? " is-active" : "")}
            onClick={() => builder.guardedNavigate("/roles")}
          >
            Roles
          </button>
          {["Audit logs", "Single sign-on"].map((it) => (
            <button key={it} className="rs-panel-item" disabled title="Coming soon">
              {it}
            </button>
          ))}
        </div>
      </div>
    </aside>
  );
}

function groupComponentsByCategory(
  components: Component[],
): { category: string; items: Component[] }[] {
  const map = new Map<string, Component[]>();
  for (const c of components) {
    const dot = c.uid.indexOf(".");
    const cat = dot >= 0 ? c.uid.slice(0, dot) : "other"; // no dot → uncategorized
    if (!map.has(cat)) map.set(cat, []);
    map.get(cat)!.push(c);
  }
  return Array.from(map.entries())
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([category, items]) => ({ category, items }));
}

function categoryLabel(key: string): string {
  return key
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
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
  const [singleModalOpen, setSingleModalOpen] = useState(false);
  const [confirmPatch, setConfirmPatch] = useState<PatchContentType | null>(null);
  const [createComponentOpen, setCreateComponentOpen] = useState(false);
  const [compRefetchKey, setCompRefetchKey] = useState(0);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const toggleCategory = (cat: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(cat) ? next.delete(cat) : next.add(cat);
      return next;
    });
  const builder = useBuilderDraft();
  // Refetch only when entering this section (base) or after a save — not on
  // every collection click, which just changes the path suffix.
  const { data: types, loading, error } = useResource(
    () => listContentTypes(),
    [base, builder.saveNonce],
  );
  const collectionTypes = types?.filter((t) => t.kind === "collection") ?? [];
  const singleTypes = types?.filter((t) => t.kind === "single") ?? [];
  const { data: components, loading: compLoading, error: compError } = useResource(
    () => listComponents(),
    [base, builder.saveNonce, compRefetchKey],
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
            className={"rs-btn rs-btn--primary rs-btn--sm rs-panel-save" + (builder.saving ? " is-saving" : "")}
            disabled={!builder.dirty || builder.saving}
            onClick={onSaveClick}
            title={builder.dirty ? "Save schema changes" : "No unsaved changes"}
          >
            {builder.saving
              ? <><Icons.spinner size={14} className="rs-spin" /> Saving…</>
              : <><Icons.save size={14} /> Save</>}
          </button>
        )}
      </div>
      {isBuilder && builder.dirty && !builder.saving && (
        <div className="rs-panel-dirty">
          <span className="rs-dot" /> Unsaved schema changes
        </div>
      )}
      <div className="rs-panel-scroll">
        <div className="rs-panel-search">
          <Icons.search size={15} />
          <input placeholder="Search types" disabled />
        </div>
        <PanelGroup
          label="Collection types"
          count={collectionTypes.length}
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
          {collectionTypes.map((t) => (
            <button
              key={t.name}
              onClick={() => builder.guardedNavigate(`${base}/${t.name}`)}
              className={"rs-panel-item rs-panel-item--btn" + (collection === t.name ? " is-active" : "")}
            >
              {t.display_name}
            </button>
          ))}
        </PanelGroup>
        <PanelGroup
          label="Single types"
          count={singleTypes.length}
          action={isBuilder}
          onAction={() => {
            if (builder.dirty) {
              if (!window.confirm("You have unsaved changes. Discard them and create a new type?")) return;
              builder.reset();
            }
            setSingleModalOpen(true);
          }}
        >
          {loading && !types &&
            [60, 44].map((w, i) => (
              <div key={i} className="rs-skel" style={{ width: `${w}%` }} />
            ))}
          {error && !types && <div className="rs-panel-item rs-danger">Failed to load</div>}
          {singleTypes.map((t) => (
            <button
              key={t.name}
              onClick={() => builder.guardedNavigate(`${isBuilder ? "/builder" : "/content/single"}/${t.name}`)}
              className={"rs-panel-item rs-panel-item--btn" + (collection === t.name ? " is-active" : "")}
            >
              {t.display_name}
            </button>
          ))}
        </PanelGroup>
        {isBuilder && (
          <>
          <PanelGroup
            label="Components"
            count={components?.length ?? 0}
            action
            onAction={() => {
              if (builder.dirty) {
                if (!window.confirm("You have unsaved changes. Discard them?")) return;
                builder.reset();
              }
              setCreateComponentOpen(true);
            }}
          >
            {compLoading && !components &&
              [60, 44, 52].map((w, i) => (
                <div key={i} className="rs-skel" style={{ width: `${w}%` }} />
              ))}
            {compError && !components && (
              <div className="rs-panel-item rs-danger">Failed to load</div>
            )}
            {components && groupComponentsByCategory(components).map(({ category, items }) => (
              <div key={category}>
                <button
                  className="rs-panel-grouphead"
                  onClick={() => toggleCategory(category)}
                  style={{ width: "100%", background: "none", border: "none", cursor: "pointer" }}
                >
                  <span>{categoryLabel(category)}</span>
                  <Icons.chevDown
                    size={13}
                    style={{
                      transform: collapsed.has(category) ? "rotate(-90deg)" : "none",
                      transition: "transform .14s",
                      flexShrink: 0,
                    }}
                  />
                </button>
                {!collapsed.has(category) && items.map((c) => (
                  <button
                    key={c.uid}
                    className={
                      "rs-panel-item rs-panel-item--btn" +
                      (location.pathname === `/builder/components/${encodeURIComponent(c.uid)}` ? " is-active" : "")
                    }
                    onClick={() => builder.guardedNavigate(`/builder/components/${encodeURIComponent(c.uid)}`)}
                  >
                    {c.display_name}
                  </button>
                ))}
              </div>
            ))}
          </PanelGroup>
          </>
        )}
      </div>
      {modalOpen && <CreateTypeModal onClose={() => setModalOpen(false)} />}
      {singleModalOpen && <CreateSingleTypeModal onClose={() => setSingleModalOpen(false)} />}
      {createComponentOpen && (
        <CreateComponentModal
          onClose={() => setCreateComponentOpen(false)}
          onCreated={(uid) => {
            setCreateComponentOpen(false);
            setCompRefetchKey((k) => k + 1);
            builder.guardedNavigate(`/builder/components/${encodeURIComponent(uid)}`);
          }}
        />
      )}
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
            initials={initials(email ?? "Admin")}
            color={AVATAR_NEUTRAL}
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
