import { Fragment, useEffect, useMemo, useState } from "react";
import { listAudit, auditStats, auditExportPath } from "../api/audit";
import type { AuditRow, AuditFilters } from "../api/audit";
import { Icons, type IconKey } from "../components/icons";
import { Avatar } from "../components/shell";
import { StatCard } from "../components/StatCard";
import { LoadingState, EmptyState } from "../components/ui";
import { fetchBlob } from "../api/client";
import { useResource } from "../hooks/useResource";
import { relTime, initials, AVATAR_NEUTRAL } from "../util";

/* ---- Action vocabulary ---- */
const AUDIT_CATS: Record<string, { label: string; color: string }> = {
  content: { label: "Content", color: "#0E7490" },
  auth: { label: "Authentication", color: "#B45309" },
  settings: { label: "Settings", color: "#475569" },
  perm: { label: "Permissions", color: "#7C3AED" },
};

type ActionDef = { label: string; icon: IconKey; cat: string; danger?: boolean };

const AUDIT_ACTIONS: Record<string, ActionDef> = {
  "entry.create": { label: "Created entry", icon: "plus", cat: "content" },
  "entry.update": { label: "Updated entry", icon: "edit", cat: "content" },
  "entry.delete": { label: "Deleted entry", icon: "trash", cat: "content", danger: true },
  "entry.publish": { label: "Published entry", icon: "eye", cat: "content" },
  "entry.unpublish": { label: "Unpublished entry", icon: "x", cat: "content" },
  "auth.login": { label: "Signed in", icon: "lock", cat: "auth" },
  "auth.login_failed": { label: "Failed sign-in", icon: "x", cat: "auth", danger: true },
  "token.create": { label: "Created API token", icon: "plus", cat: "settings" },
  "token.revoke": { label: "Revoked API token", icon: "trash", cat: "settings", danger: true },
  "webhook.create": { label: "Created webhook", icon: "link", cat: "settings" },
  "settings.update": { label: "Updated settings", icon: "gear", cat: "settings" },
  "role.change": { label: "Changed role", icon: "gear", cat: "perm" },
  "user.invite": { label: "Invited user", icon: "user", cat: "perm" },
  "user.suspend": { label: "Suspended user", icon: "lock", cat: "perm", danger: true },
};

const TARGET_LABEL: Record<string, string> = {
  article: "Article",
  user: "User",
  token: "Token",
  webhook: "Webhook",
  role: "Role",
  session: "Session",
  settings: "Settings",
};

function absTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

/* ---- Action cell: tinted category chip + verb ---- */
function ActionCell({ action }: { action: string }) {
  const def = AUDIT_ACTIONS[action];
  if (!def) {
    return <span className="rs-audit-verb">{action}</span>;
  }
  const cat = AUDIT_CATS[def.cat];
  const I = Icons[def.icon];
  return (
    <span className="rs-audit-action">
      <span className="rs-audit-icon" style={{ "--chip": cat.color } as React.CSSProperties}>
        <I size={15} />
      </span>
      <span className="rs-audit-verb">{def.label}</span>
    </span>
  );
}

/* ---- Expandable detail row ---- */
function AuditDetail({ ev }: { ev: AuditRow }) {
  const meta: [string, string, boolean][] = [
    ["Timestamp", absTime(ev.created_at), false],
    ["Actor", ev.actor_label, false],
    ["IP address", ev.ip ?? "—", true],
    ["Location", "—", false],
    ["Device", ev.user_agent ?? "—", false],
    ["Request ID", ev.request_id ?? "—", true],
  ];
  return (
    <div className="rs-audit-detail">
      <div className="rs-audit-detail-grid">
        {meta.map(([k, v, mono]) => (
          <div className="rs-audit-meta" key={k}>
            <span className="rs-audit-meta-k">{k}</span>
            <span className={"rs-audit-meta-v" + (mono ? " rs-mono" : "")}>{v}</span>
          </div>
        ))}
      </div>

      {ev.changes && ev.changes.length > 0 && (
        <div className="rs-audit-changes">
          <span className="rs-audit-changes-head">Changes</span>
          {ev.changes.map((c, i) => (
            <div className="rs-audit-change" key={i}>
              <code className="rs-mono rs-audit-field">{c.field}</code>
              <span className="rs-audit-from">{c.from}</span>
              <Icons.chevRight size={13} className="rs-audit-arrow" />
              <span className="rs-audit-to">{c.to}</span>
            </div>
          ))}
        </div>
      )}

      {ev.note && (
        <p className="rs-audit-note">
          <Icons.bolt size={13} /> {ev.note}
        </p>
      )}

      <div className="rs-audit-detail-foot">
        <code className="rs-mono rs-audit-event">{ev.action}</code>
        <span className="rs-audit-detail-id rs-mono">{ev.id}</span>
      </div>
    </div>
  );
}

type ActorOpt = { id: string; label: string };

/* ---- Actor filter popover (distinct actors from loaded rows) ---- */
function ActorFilter({
  value,
  options,
  onChange,
}: {
  value: string | null;
  options: ActorOpt[];
  onChange: (id: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const sel = value ? options.find((a) => a.id === value) : null;
  return (
    <div className="rs-pop-anchor">
      <button
        className={"rs-btn rs-btn--ghost" + (value ? " is-active" : "")}
        onClick={() => setOpen((o) => !o)}
      >
        <Icons.user size={15} /> {sel ? sel.label : "Actor"}
      </button>
      {open && (
        <>
          <div className="rs-pop-scrim" onClick={() => setOpen(false)} />
          <div className="rs-pop" role="menu">
            <div className="rs-pop-head">
              <strong>Filter by actor</strong>
              {value && (
                <button
                  className="rs-link-btn"
                  onClick={() => {
                    onChange(null);
                    setOpen(false);
                  }}
                >
                  Clear
                </button>
              )}
            </div>
            <div className="rs-pop-body rs-audit-actorlist">
              <button
                className={"rs-audit-actor-opt" + (!value ? " is-active" : "")}
                onClick={() => {
                  onChange(null);
                  setOpen(false);
                }}
              >
                <span className="rs-audit-actor-all">
                  <Icons.user size={14} />
                </span>
                <span>All actors</span>
                {!value && <Icons.check size={14} className="rs-audit-actor-check" />}
              </button>
              {options.map((a) => (
                <button
                  key={a.id}
                  className={"rs-audit-actor-opt" + (value === a.id ? " is-active" : "")}
                  onClick={() => {
                    onChange(a.id);
                    setOpen(false);
                  }}
                >
                  <Avatar name={a.label} initials={initials(a.label)} color={AVATAR_NEUTRAL} size={22} />
                  <span>{a.label}</span>
                  {value === a.id && <Icons.check size={14} className="rs-audit-actor-check" />}
                </button>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

const TABS: [string, string][] = [
  ["all", "All"],
  ["content", "Content"],
  ["auth", "Authentication"],
  ["settings", "Settings"],
  ["perm", "Permissions"],
];

export function AuditLog() {
  const [tab, setTab] = useState("all");
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [actor, setActor] = useState<string | null>(null);
  const [statusF, setStatusF] = useState("all");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [perPage, setPerPage] = useState(25);

  // Debounce search like ContentList does.
  useEffect(() => {
    const t = setTimeout(() => setDebouncedQuery(query), 300);
    return () => clearTimeout(t);
  }, [query]);

  // Reset to first page when filters change.
  useEffect(() => {
    setPage(1);
  }, [tab, debouncedQuery, actor, statusF, perPage]);

  const filters: AuditFilters = {
    category: tab === "all" ? undefined : tab,
    status: statusF === "all" ? undefined : statusF,
    actor_id: actor || undefined,
    q: debouncedQuery || undefined,
    page,
    per_page: perPage,
  };

  const list = useResource(
    () => listAudit(filters),
    [tab, statusF, actor, debouncedQuery, page, perPage],
  );
  const stats = useResource(() => auditStats(), []);

  const rows = list.data?.rows ?? [];
  const total = list.data?.total ?? 0;
  const catCounts = list.data?.category_counts ?? {};

  // Distinct actors seen in the currently loaded rows.
  const actorOpts = useMemo<ActorOpt[]>(() => {
    const seen = new Map<string, string>();
    for (const r of rows) {
      if (r.actor_id && !seen.has(r.actor_id)) seen.set(r.actor_id, r.actor_label);
    }
    return [...seen.entries()].map(([id, label]) => ({ id, label }));
  }, [rows]);

  const tabCount = (k: string) =>
    k === "all"
      ? Object.values(catCounts).reduce((a, b) => a + b, 0)
      : catCounts[k] ?? 0;

  const pageCount = Math.max(1, Math.ceil(total / perPage));
  const pageWindow: number[] = [];
  for (let p = 1; p <= pageCount; p++) pageWindow.push(p);

  const handleExport = async () => {
    try {
      const blob = await fetchBlob(auditExportPath(filters));
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "audit-log.csv";
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch {
      // network error — ignore
    }
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Audit logs</h1>
          <p className="rs-cm-sub">
            An immutable record of every action across the workspace. Retained for 90 days.
          </p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={handleExport}>
          <Icons.external size={16} /> Export log
        </button>
      </div>

      <div className="rs-stat-grid rs-audit-stats">
        <StatCard
          label="Events logged"
          value={stats.data?.events_logged ?? "—"}
          delta="last 90 days"
          icon="clock"
          tone="accent"
        />
        <StatCard
          label="Sign-ins"
          value={stats.data?.sign_ins ?? "—"}
          delta={`${stats.data?.failed_attempts ?? 0} failed attempts`}
          icon="lock"
          tone="ok"
        />
        <StatCard
          label="Content changes"
          value={stats.data?.content_changes ?? "—"}
          delta="create · edit · publish"
          icon="doc"
          tone="muted"
        />
        <StatCard
          label="Failed actions"
          value={stats.data?.failed_actions ?? "—"}
          delta="review recommended"
          icon="gear"
          tone="warn"
        />
      </div>

      <div className="rs-cm-tabs">
        {TABS.map(([k, l]) => (
          <button
            key={k}
            className={"rs-tab" + (tab === k ? " is-active" : "")}
            onClick={() => setTab(k)}
          >
            {l} <span className="rs-tab-count">{tabCount(k)}</span>
          </button>
        ))}
      </div>

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input
            placeholder="Search by actor, action, or target"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <ActorFilter value={actor} options={actorOpts} onChange={setActor} />
        <div className="rs-spacer" />
        <div className="rs-segment">
          {[
            ["all", "All"],
            ["success", "Success"],
            ["failed", "Failed"],
          ].map(([k, l]) => (
            <button
              key={k}
              className={"rs-seg" + (statusF === k ? " is-active" : "")}
              onClick={() => setStatusF(k)}
            >
              {l}
            </button>
          ))}
        </div>
      </div>

      <div className="rs-table-wrap rs-audit-wrap">
        <table className="rs-table rs-audit-table">
          <thead>
            <tr>
              <th className="rs-audit-col-time">Time</th>
              <th>Actor</th>
              <th>Action</th>
              <th>Target</th>
              <th className="rs-audit-col-ctx">Context</th>
              <th className="rs-col-act"></th>
            </tr>
          </thead>
          <tbody>
            {rows.map((e) => {
              const open = expanded === e.id;
              return (
                <Fragment key={e.id}>
                  <tr
                    className={open ? "is-selected" : ""}
                    onClick={() => setExpanded(open ? null : e.id)}
                  >
                    <td
                      className="rs-cell-muted rs-audit-time"
                      title={absTime(e.created_at)}
                    >
                      {relTime(e.created_at)}
                    </td>
                    <td>
                      <span className="rs-cell-author">
                        <Avatar
                          name={e.actor_label}
                          initials={initials(e.actor_label)}
                          color={AVATAR_NEUTRAL}
                          size={22}
                        />
                        {e.actor_label}
                      </span>
                    </td>
                    <td>
                      <ActionCell action={e.action} />
                    </td>
                    <td>
                      <span className="rs-audit-target">
                        {e.target_type && (
                          <span className="rs-type-pill">
                            {TARGET_LABEL[e.target_type] ?? e.target_type}
                          </span>
                        )}
                        <span className="rs-audit-target-name" title={e.target_label ?? ""}>
                          {e.target_label ?? "—"}
                        </span>
                      </span>
                    </td>
                    <td>
                      <span className="rs-audit-ctx">
                        <span className={"rs-audit-status rs-audit-status--" + e.status}>
                          <span className="rs-dot" />
                          {e.status === "failed" ? "Failed" : "Success"}
                        </span>
                        <code className="rs-mono rs-audit-ip">{e.ip ?? "—"}</code>
                      </span>
                    </td>
                    <td className="rs-col-act">
                      <button className="rs-row-btn rs-audit-expand" tabIndex={-1}>
                        <Icons.chevDown
                          size={16}
                          className={open ? "rs-audit-chev is-open" : "rs-audit-chev"}
                        />
                      </button>
                    </td>
                  </tr>
                  {open && (
                    <tr className="rs-audit-detail-row" onClick={(e2) => e2.stopPropagation()}>
                      <td colSpan={6}>
                        <AuditDetail ev={e} />
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
          </tbody>
        </table>
        {list.loading && rows.length === 0 && <LoadingState />}
        {list.error && (
          <EmptyState>
            {list.error.message}{" "}
            <button className="rs-link-btn" onClick={list.refetch}>
              Retry
            </button>
          </EmptyState>
        )}
        {!list.loading && !list.error && rows.length === 0 && (
          <EmptyState>No events match your filters.</EmptyState>
        )}
      </div>

      <div className="rs-pager">
        <span className="rs-cell-muted">
          Showing {rows.length} of {total} events
        </span>
        <div className="rs-pager-ctrl">
          <button
            className="rs-page-btn"
            disabled={page <= 1}
            onClick={() => setPage(page - 1)}
          >
            <Icons.chevLeft size={16} />
          </button>
          {pageWindow.map((p) => (
            <button
              key={p}
              className={"rs-page-btn" + (p === page ? " is-active" : "")}
              onClick={() => setPage(p)}
            >
              {p}
            </button>
          ))}
          <button
            className="rs-page-btn"
            disabled={page >= pageCount}
            onClick={() => setPage(page + 1)}
          >
            <Icons.chevRight size={16} />
          </button>
          <select
            className="rs-select-sm"
            value={perPage}
            onChange={(e) => setPerPage(Number(e.target.value))}
          >
            <option value={25}>25 / page</option>
            <option value={50}>50 / page</option>
          </select>
        </div>
      </div>
    </div>
  );
}
