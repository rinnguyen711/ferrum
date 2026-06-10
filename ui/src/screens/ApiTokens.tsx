import { useMemo, useState } from "react";
import { useNavigate, useLocation, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState, Notice } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listApiTokens, listContentTypes, createApiToken, revokeApiToken, updateApiToken } from "../api/endpoints";
import type { ApiToken, ContentType, NewApiToken } from "../api/types";
import { relTime } from "../util";
import { ApiError } from "../api/client";

// ─── Permission model ─────────────────────────────────────────────────────────

// Granular actions shown in the UI (matches the design's permission matrix).
const PERM_ACTIONS = ["find", "findOne", "create", "update", "delete"] as const;
type PermAction = typeof PERM_ACTIONS[number];

// Per content-type: which actions are enabled.
type PermMap = Record<string, Set<PermAction>>;

// Map UI actions → backend scope verbs.
function actionToScope(a: PermAction): "read" | "write" | "delete" {
  if (a === "find" || a === "findOne") return "read";
  if (a === "create" || a === "update") return "write";
  return "delete";
}

// Convert per-type action sets → scope strings for the API.
// Produces per-type scopes (e.g. "content:read:article") or wildcard ("content:read")
// if all types have the same verb enabled.
function permMapToScopes(map: PermMap, ctNames: string[]): string[] {
  const verbs = ["read", "write", "delete"] as const;
  const out: string[] = [];
  for (const verb of verbs) {
    // Which actions belong to this verb?
    const verbActions = PERM_ACTIONS.filter((a) => actionToScope(a) === verb);
    const typesWithVerb = ctNames.filter((ct) => {
      const s = map[ct] ?? new Set();
      return verbActions.some((a) => s.has(a));
    });
    if (typesWithVerb.length === 0) continue;
    if (typesWithVerb.length === ctNames.length && ctNames.length > 0) {
      out.push(`content:${verb}`);
    } else {
      typesWithVerb.forEach((ct) => out.push(`content:${verb}:${ct}`));
    }
  }
  return out;
}

function seedPermMap(preset: TokenPreset, ctNames: string[]): PermMap {
  const out: PermMap = {};
  for (const ct of ctNames) {
    if (preset === "read-only") out[ct] = new Set(["find", "findOne"]);
    else if (preset === "full") out[ct] = new Set([...PERM_ACTIONS]);
    else out[ct] = new Set();
  }
  return out;
}

type TokenPreset = "read-only" | "full" | "custom";

function fmtExpiry(iso: string): string {
  const d = new Date(iso);
  const days = Math.round((d.getTime() - Date.now()) / 864e5);
  if (days < 0) return "Expired";
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days < 30) return `${days}d`;
  const months = Math.round(days / 30);
  return `${months}mo`;
}

function inferPreset(scopes: string[]): TokenPreset {
  const verbs = scopes.map((s) => s.split(":")[1]).filter(Boolean);
  const hasWrite = verbs.includes("write");
  const hasDelete = verbs.includes("delete");
  const allWild = scopes.every((s) => s.split(":").length === 2);
  if (!hasWrite && !hasDelete) return "read-only";
  if (hasWrite && hasDelete && allWild) return "full";
  return "custom";
}

const TOKEN_PRESETS: {
  key: TokenPreset; name: string; icon: keyof typeof Icons; desc: string;
}[] = [
  { key: "read-only", name: "Read-only",   icon: "eye",    desc: "Can fetch entries from every collection. No writes." },
  { key: "full",      name: "Full access", icon: "bolt",   desc: "Every action on every collection type." },
  { key: "custom",    name: "Custom",      icon: "lock",   desc: "Hand-pick exactly which routes the token may call." },
];

const DURATIONS = [
  { key: "7d",   label: "7 days",    days: 7   },
  { key: "30d",  label: "30 days",   days: 30  },
  { key: "90d",  label: "90 days",   days: 90  },
  { key: "none", label: "Unlimited", days: null },
];

// ─── Token list ──────────────────────────────────────────────────────────────

export function ApiTokens() {
  const navigate = useNavigate();
  const tokens = useResource(() => listApiTokens(), []);
  const [revokeTarget, setRevokeTarget] = useState<ApiToken | null>(null);

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>API Tokens</h1>
          <p className="rs-cm-sub">{(tokens.data ?? []).length} token{(tokens.data ?? []).length === 1 ? "" : "s"}</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => navigate("/settings/api-tokens/new")}>
          <Icons.plus size={16} /> Create token
        </button>
      </div>

      {tokens.loading && <LoadingState />}
      {tokens.error && <EmptyState>{tokens.error.message}</EmptyState>}

      {!tokens.loading && !tokens.error && (
        <div className="rs-table-wrap">
          <table className="rs-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Description</th>
                <th>Type</th>
                <th>Expires</th>
                <th>Last used</th>
                <th className="rs-col-act" />
              </tr>
            </thead>
            <tbody>
              {(tokens.data ?? []).map((t) => {
                const preset = inferPreset(t.scopes);
                const presetMeta = TOKEN_PRESETS.find((p) => p.key === preset)!;
                return (
                  <tr
                    key={t.id}
                    className="rs-row-link"
                    style={{ cursor: "pointer" }}
                    onClick={() => navigate(`/settings/api-tokens/${t.id}`, { state: { token: t } })}
                  >
                    <td className="rs-cell-title">
                      <span className="rs-title-text">{t.name}</span>
                    </td>
                    <td className="rs-cell-muted">{t.description || "—"}</td>
                    <td>
                      <span className="rs-type-pill">{presetMeta.name}</span>
                    </td>
                    <td className="rs-cell-muted">
                      {t.expires_at
                        ? new Date(t.expires_at) < new Date()
                          ? <span className="rs-badge rs-badge--warn">Expired</span>
                          : fmtExpiry(t.expires_at)
                        : "Never"}
                    </td>
                    <td className="rs-cell-muted">{t.last_used_at ? relTime(t.last_used_at) : "—"}</td>
                    <td className="rs-col-act">
                      <button
                        className="rs-row-btn rs-danger"
                        title="Revoke token"
                        onClick={(e) => { e.stopPropagation(); setRevokeTarget(t); }}
                      >
                        <Icons.trash size={16} />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {(tokens.data ?? []).length === 0 && (
            <div className="rs-empty">No API tokens yet. Create one to allow external access.</div>
          )}
        </div>
      )}

      {revokeTarget && (
        <RevokeModal
          token={revokeTarget}
          onRevoked={() => { setRevokeTarget(null); tokens.refetch(); }}
          onClose={() => setRevokeTarget(null)}
        />
      )}
    </div>
  );
}

// ─── Token editor (full page) ────────────────────────────────────────────────

export function TokenEditor() {
  const navigate = useNavigate();
  const contentTypes = useResource(() => listContentTypes(), []);
  const cts = (contentTypes.data ?? []).filter((ct) => ct.kind === "collection");
  const ctNames = cts.map((ct) => ct.name);

  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [preset, setPreset] = useState<TokenPreset>("read-only");
  const [duration, setDuration] = useState("90d");
  const [permMap, setPermMap] = useState<PermMap>({});
  const [openType, setOpenType] = useState<Record<string, boolean>>({});
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  // Re-seed perm map when content types load or a non-custom preset is explicitly chosen.
  // Switching to "custom" keeps the existing map intact (user is editing from a preset base).
  const ctsKey = ctNames.join(",");
  useMemo(() => {
    if (created) return;
    if (preset !== "custom") setPermMap(seedPermMap(preset, ctNames));
    if (cts.length > 0 && Object.keys(openType).length === 0) {
      setOpenType({ [ctNames[0]]: true });
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [preset, ctsKey, created]);

  const dur = DURATIONS.find((d) => d.key === duration)!;
  const presetMeta = TOKEN_PRESETS.find((p) => p.key === preset)!;
  const isCustom = preset === "custom";

  const totalEnabled = Object.values(permMap).reduce((n, s) => n + s.size, 0);
  const totalActions = ctNames.length * PERM_ACTIONS.length;

  const scopeStrings = useMemo(() => permMapToScopes(permMap, ctNames), [permMap, ctsKey]);
  const valid = name.trim().length > 0 && scopeStrings.length > 0;

  const expiryText = useMemo(() => {
    if (!dur.days) return "Never expires";
    const d = new Date(Date.now() + dur.days * 864e5);
    return "Expires " + d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  }, [duration]);

  const choosePreset = (key: TokenPreset) => {
    setPreset(key);
    setDirty(true);
  };

  const toggleAction = (ctName: string, action: PermAction) => {
    if (!isCustom) setPreset("custom");
    setPermMap((prev) => {
      const next = new Set(prev[ctName] ?? new Set<PermAction>());
      next.has(action) ? next.delete(action) : next.add(action);
      return { ...prev, [ctName]: next };
    });
    setDirty(true);
  };

  const toggleAllForType = (ctName: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!isCustom) setPreset("custom");
    setPermMap((prev) => {
      const current = prev[ctName] ?? new Set<PermAction>();
      const isFull = current.size === PERM_ACTIONS.length;
      return { ...prev, [ctName]: new Set(isFull ? [] : [...PERM_ACTIONS]) };
    });
    setDirty(true);
  };

  const copy = async () => {
    if (!created) return;
    await navigator.clipboard.writeText(created).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1600);
  };

  const submit = async () => {
    setError(null);
    setSaving(true);
    try {
      const body: NewApiToken = {
        name: name.trim(),
        description: desc.trim(),
        scopes: scopeStrings,
      };
      if (dur.days) {
        body.expires_at = new Date(Date.now() + dur.days * 864e5).toISOString();
      }
      const result = await createApiToken(body);
      setCreated(result.token);
      setDirty(false);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to create token.");
    } finally {
      setSaving(false);
    }
  };

  const back = () => navigate("/settings/api-tokens");

  return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={back}><Icons.arrowLeft size={18} /></button>
        <div className="rs-editor-titlewrap">
          <h1>{created ? (name.trim() || "Untitled token") : "Create API token"}</h1>
          <div className="rs-editor-meta">
            <span className="rs-type-pill">{presetMeta.name}</span>
            <span className="rs-cell-muted">· {dur.label}</span>
            {!created && dirty && <span className="rs-unsaved"><span className="rs-dot" /> Unsaved</span>}
            {created && <span className="rs-cell-muted">· {totalEnabled}/{totalActions} permissions</span>}
          </div>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" onClick={back}>{created ? "Done" : "Cancel"}</button>
          {!created && (
            <button className="rs-btn rs-btn--primary" disabled={!valid || saving} onClick={submit}>
              <Icons.bolt size={15} /> {saving ? "Creating…" : "Create token"}
            </button>
          )}
        </div>
      </div>

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          {error && <div style={{ marginBottom: 18 }}><Notice>{error}</Notice></div>}

          {created && (
            <div className="rs-token-reveal rs-token-reveal--success">
              <div className="rs-token-reveal-head">
                <span className="rs-token-reveal-icon"><Icons.check size={16} /></span>
                <div>
                  <strong>Token created</strong>
                  <p>Copy it now — for security, Rustapi only shows the full token once.</p>
                </div>
              </div>
              <div className="rs-input-affix rs-token-key">
                <span className="rs-affix"><Icons.lock size={14} /></span>
                <input className="rs-input rs-mono" readOnly value={created} onFocus={(e) => e.target.select()} />
                <button className="rs-affix-btn" onClick={copy} title="Copy token">
                  {copied ? <Icons.check size={15} /> : <Icons.copy size={15} />}
                </button>
              </div>
            </div>
          )}

          <div className="rs-fields rs-fields--token">
            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Name <span className="rs-label-req">*</span></label>
              <input
                className="rs-input rs-input--lg"
                value={name}
                disabled={!!created}
                placeholder="e.g. Production read-only"
                onChange={(e) => { setName(e.target.value); setDirty(true); }}
              />
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Description <span className="rs-label-hint">— What this token is used for</span></label>
              <textarea
                className="rs-input rs-textarea"
                rows={2}
                value={desc}
                disabled={!!created}
                placeholder="Describe where this token is used and by whom."
                onChange={(e) => { setDesc(e.target.value); setDirty(true); }}
              />
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Token duration <span className="rs-label-hint">— {expiryText}</span></label>
              <div className="rs-segment rs-segment--lg">
                {DURATIONS.map((d) => (
                  <button key={d.key} className={"rs-seg" + (duration === d.key ? " is-active" : "")}
                    disabled={!!created} onClick={() => { setDuration(d.key); setDirty(true); }}>
                    {d.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Token type <span className="rs-label-req">*</span></label>
              <div className="rs-radio-cards rs-radio-cards--3">
                {TOKEN_PRESETS.map((p) => {
                  const I = Icons[p.icon] ?? Icons.lock;
                  return (
                    <button key={p.key}
                      className={"rs-radio-card rs-token-card" + (preset === p.key ? " is-on" : "")}
                      disabled={!!created} onClick={() => choosePreset(p.key)}>
                      <span className="rs-radio-dot" />
                      <span className="rs-radio-text">
                        <strong><I size={14} /> {p.name}</strong>
                        <span>{p.desc}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">
                Permissions
                <span className="rs-label-hint">
                  {` — ${totalEnabled} of ${totalActions} actions enabled`}
                </span>
              </label>
              <div className="rs-perm-list">
                {contentTypes.loading && (
                  <div className="rs-perm-type-body" style={{ padding: "12px 14px" }}>
                    <span className="rs-cell-muted" style={{ fontSize: 12 }}>Loading content types…</span>
                  </div>
                )}
                {cts.map((ct) => {
                  const enabled = permMap[ct.name] ?? new Set<PermAction>();
                  const isFull = enabled.size === PERM_ACTIONS.length;
                  const isOpen = !!openType[ct.name];
                  return (
                    <div key={ct.name} className={"rs-perm-type" + (isOpen ? " is-open" : "")}>
                      <button className="rs-perm-type-head" onClick={() => setOpenType((o) => ({ ...o, [ct.name]: !o[ct.name] }))}>
                        <span className="rs-perm-chev"><Icons.chevRight size={16} /></span>
                        <span className="rs-perm-type-meta">
                          <strong>{ct.display_name}</strong>
                          <code className="rs-mono">{ct.name}</code>
                        </span>
                        <span className={"rs-perm-tally" + (isFull ? " is-full" : "")}>{enabled.size}/{PERM_ACTIONS.length}</span>
                        <span className="rs-btn rs-btn--ghost rs-btn--sm" role="button"
                          onClick={(e) => toggleAllForType(ct.name, e)} style={{ marginLeft: 4 }}>
                          {isFull ? "Clear" : "All"}
                        </span>
                      </button>
                      {isOpen && (
                        <div className="rs-perm-type-body">
                          {PERM_ACTIONS.map((action) => {
                            const on = enabled.has(action);
                            return (
                              <div className="rs-perm-action" key={action}>
                                <input type="checkbox" checked={on} disabled={!!created}
                                  onChange={() => toggleAction(ct.name, action)} />
                                <label className="rs-mono" onClick={() => !created && toggleAction(ct.name, action)}>{action}</label>
                                <span className="rs-perm-scope-key">content:{actionToScope(action)}</span>
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  );
                })}
                {cts.length === 0 && !contentTypes.loading && (
                  <div className="rs-perm-type-body" style={{ padding: "12px 14px" }}>
                    <span className="rs-cell-muted" style={{ fontSize: 12 }}>No collection types yet.</span>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

        <aside className="rs-editor-rail">
          <div className="rs-rail-card">
            <div className="rs-rail-card-head"><Icons.lock size={15} /> Token</div>
            <div className="rs-rail-field">
              <label>Type</label>
              <span className="rs-type-pill" style={{ alignSelf: "flex-start" }}>{presetMeta.name}</span>
            </div>
            <div className="rs-rail-divider" />
            <div className="rs-rail-stat"><span>Permissions</span><strong>{totalEnabled} / {totalActions}</strong></div>
            <div className="rs-rail-stat"><span>Duration</span><strong>{dur.label}</strong></div>
            <div className="rs-rail-stat"><span>Status</span><strong>{created ? "Active" : "Draft"}</strong></div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head"><Icons.bolt size={15} /> How it works</div>
            <p className="rs-rail-note">
              Tokens authenticate requests with an <code className="rs-mono">Authorization: Bearer</code> header.
              Scopes are checked per-request — revoke anytime from this page.
            </p>
            <div className="rs-rail-divider" />
            <div className="rs-rail-stat"><span>Expiry</span><strong>{dur.days ? dur.days + " days" : "Never"}</strong></div>
          </div>
        </aside>
      </div>
    </div>
  );
}

// ─── Token detail (editable) ─────────────────────────────────────────────────

function scopesToPermMap(scopes: string[], ctNames: string[]): PermMap {
  const map: PermMap = {};
  for (const ct of ctNames) map[ct] = new Set();
  for (const scope of scopes) {
    const parts = scope.split(":");
    const verb = parts[1] as "read" | "write" | "delete";
    const scopeType = parts[2] ?? null;
    const targets = scopeType ? [scopeType] : ctNames;
    for (const ct of targets) {
      if (!map[ct]) map[ct] = new Set();
      for (const action of PERM_ACTIONS) {
        if (actionToScope(action) === verb) map[ct].add(action);
      }
    }
  }
  return map;
}

export function TokenDetail() {
  const navigate = useNavigate();
  const location = useLocation();
  const { id } = useParams<{ id: string }>();

  const locationToken = (location.state as { token?: ApiToken } | null)?.token ?? null;
  const list = useResource(() => (locationToken ? Promise.resolve(null) : listApiTokens()), [locationToken]);
  const token: ApiToken | null = locationToken ?? (list.data?.find((t) => t.id === id) ?? null);

  const contentTypes = useResource(() => listContentTypes(), []);
  const cts = (contentTypes.data ?? []).filter((ct) => ct.kind === "collection");
  const ctNames = cts.map((ct) => ct.name);
  const ctsKey = ctNames.join(",");

  // Editable state — seeded from token once available.
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [preset, setPreset] = useState<TokenPreset>("read-only");
  const [duration, setDuration] = useState("none");
  const [permMap, setPermMap] = useState<PermMap>({});
  const [openType, setOpenType] = useState<Record<string, boolean>>({});
  const [dirty, setDirty] = useState(false);
  const [seeded, setSeeded] = useState(false);

  // Seed editable fields once token + content types are both available.
  useMemo(() => {
    if (!token || seeded) return;
    setName(token.name);
    setDesc(token.description ?? "");
    const p = inferPreset(token.scopes);
    setPreset(p);
    // Infer duration from expires_at.
    if (!token.expires_at) {
      setDuration("none");
    } else {
      const daysLeft = Math.round((new Date(token.expires_at).getTime() - Date.now()) / 864e5);
      if (daysLeft <= 7) setDuration("7d");
      else if (daysLeft <= 30) setDuration("30d");
      else setDuration("90d");
    }
    if (ctNames.length > 0) {
      setPermMap(scopesToPermMap(token.scopes, ctNames));
      setOpenType({ [ctNames[0]]: true });
      setSeeded(true);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token?.id, ctsKey]);

  // Re-seed permMap when a non-custom preset is chosen (same logic as TokenEditor).
  useMemo(() => {
    if (!seeded) return;
    if (preset !== "custom") setPermMap(seedPermMap(preset, ctNames));
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [preset, ctsKey]);

  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiToken | null>(null);

  const back = () => navigate("/settings/api-tokens");
  const dur = DURATIONS.find((d) => d.key === duration)!;
  const presetMeta = TOKEN_PRESETS.find((p) => p.key === preset)!;
  const isCustom = preset === "custom";

  const scopeStrings = useMemo(() => permMapToScopes(permMap, ctNames), [permMap, ctsKey]);
  const totalEnabled = Object.values(permMap).reduce((n, s) => n + s.size, 0);
  const totalActions = ctNames.length * PERM_ACTIONS.length;
  const valid = name.trim().length > 0 && scopeStrings.length > 0;

  const choosePreset = (key: TokenPreset) => { setPreset(key); setDirty(true); };

  const toggleAction = (ctName: string, action: PermAction) => {
    if (!isCustom) setPreset("custom");
    setPermMap((prev) => {
      const next = new Set(prev[ctName] ?? new Set<PermAction>());
      next.has(action) ? next.delete(action) : next.add(action);
      return { ...prev, [ctName]: next };
    });
    setDirty(true);
  };

  const toggleAllForType = (ctName: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!isCustom) setPreset("custom");
    setPermMap((prev) => {
      const current = prev[ctName] ?? new Set<PermAction>();
      const isFull = current.size === PERM_ACTIONS.length;
      return { ...prev, [ctName]: new Set(isFull ? [] : [...PERM_ACTIONS]) };
    });
    setDirty(true);
  };

  const save = async () => {
    if (!token) return;
    setError(null);
    setSaving(true);
    try {
      const body: NewApiToken = {
        name: name.trim(),
        description: desc.trim(),
        scopes: scopeStrings,
        expires_at: dur.days ? new Date(Date.now() + dur.days * 864e5).toISOString() : null,
      };
      const updated = await updateApiToken(token.id, body);
      setDirty(false);
      navigate(`/settings/api-tokens/${token.id}`, {
        replace: true,
        state: { token: updated },
      });
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to save token.");
    } finally {
      setSaving(false);
    }
  };

  if (list.loading && !token) return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={back}><Icons.arrowLeft size={18} /></button>
        <div className="rs-editor-titlewrap"><h1>API Token</h1></div>
      </div>
      <div className="rs-editor-body"><LoadingState /></div>
    </div>
  );

  if (!token) return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={back}><Icons.arrowLeft size={18} /></button>
        <div className="rs-editor-titlewrap"><h1>API Token</h1></div>
      </div>
      <div className="rs-editor-body"><EmptyState>Token not found.</EmptyState></div>
    </div>
  );

  const isExpired = token.expires_at ? new Date(token.expires_at) < new Date() : false;
  const expiryText = useMemo(() => {
    if (!dur.days) return "Never expires";
    const d = new Date(Date.now() + dur.days * 864e5);
    return "Expires " + d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  }, [duration]);

  return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={back}><Icons.arrowLeft size={18} /></button>
        <div className="rs-editor-titlewrap">
          <h1>{name || token.name}</h1>
          <div className="rs-editor-meta">
            <span className="rs-type-pill">{presetMeta.name}</span>
            {isExpired && !dirty && <span className="rs-badge rs-badge--warn">Expired</span>}
            {dirty && <span className="rs-unsaved"><span className="rs-dot" /> Unsaved</span>}
          </div>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" onClick={back}>Cancel</button>
          <button className="rs-btn rs-btn--ghost rs-danger" onClick={() => setRevokeTarget(token)}>
            <Icons.trash size={15} /> Revoke
          </button>
          <button className="rs-btn rs-btn--primary" disabled={!valid || saving} onClick={save}>
            <Icons.bolt size={15} /> {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          {error && <div style={{ marginBottom: 18 }}><Notice>{error}</Notice></div>}

          <div className="rs-fields rs-fields--token">
            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Name <span className="rs-label-req">*</span></label>
              <input
                className="rs-input rs-input--lg"
                value={name}
                placeholder="e.g. Production read-only"
                onChange={(e) => { setName(e.target.value); setDirty(true); }}
              />
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Description <span className="rs-label-hint">— What this token is used for</span></label>
              <textarea
                className="rs-input rs-textarea"
                rows={2}
                value={desc}
                placeholder="Describe where this token is used and by whom."
                onChange={(e) => { setDesc(e.target.value); setDirty(true); }}
              />
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Token duration <span className="rs-label-hint">— {expiryText}</span></label>
              <div className="rs-segment rs-segment--lg">
                {DURATIONS.map((d) => (
                  <button key={d.key} className={"rs-seg" + (duration === d.key ? " is-active" : "")}
                    onClick={() => { setDuration(d.key); setDirty(true); }}>
                    {d.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">Token type <span className="rs-label-req">*</span></label>
              <div className="rs-radio-cards rs-radio-cards--3">
                {TOKEN_PRESETS.map((p) => {
                  const I = Icons[p.icon] ?? Icons.lock;
                  return (
                    <button key={p.key}
                      className={"rs-radio-card rs-token-card" + (preset === p.key ? " is-on" : "")}
                      onClick={() => choosePreset(p.key)}>
                      <span className="rs-radio-dot" />
                      <span className="rs-radio-text">
                        <strong><I size={14} /> {p.name}</strong>
                        <span>{p.desc}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="rs-field-row rs-field-span2">
              <label className="rs-label">
                Permissions
                <span className="rs-label-hint">{` — ${totalEnabled} of ${totalActions} actions enabled`}</span>
              </label>
              <div className="rs-perm-list">
                {contentTypes.loading && (
                  <div className="rs-perm-type-body" style={{ padding: "12px 14px" }}>
                    <span className="rs-cell-muted" style={{ fontSize: 12 }}>Loading content types…</span>
                  </div>
                )}
                {cts.map((ct) => {
                  const enabled = permMap[ct.name] ?? new Set<PermAction>();
                  const isFull = enabled.size === PERM_ACTIONS.length;
                  const isOpen = !!openType[ct.name];
                  return (
                    <div key={ct.name} className={"rs-perm-type" + (isOpen ? " is-open" : "")}>
                      <button className="rs-perm-type-head" onClick={() => setOpenType((o) => ({ ...o, [ct.name]: !o[ct.name] }))}>
                        <span className="rs-perm-chev"><Icons.chevRight size={16} /></span>
                        <span className="rs-perm-type-meta">
                          <strong>{ct.display_name}</strong>
                          <code className="rs-mono">{ct.name}</code>
                        </span>
                        <span className={"rs-perm-tally" + (isFull ? " is-full" : "")}>{enabled.size}/{PERM_ACTIONS.length}</span>
                        <span className="rs-btn rs-btn--ghost rs-btn--sm" role="button"
                          onClick={(e) => toggleAllForType(ct.name, e)} style={{ marginLeft: 4 }}>
                          {isFull ? "Clear" : "All"}
                        </span>
                      </button>
                      {isOpen && (
                        <div className="rs-perm-type-body">
                          {PERM_ACTIONS.map((action) => {
                            const on = enabled.has(action);
                            return (
                              <div className="rs-perm-action" key={action}>
                                <input type="checkbox" checked={on} onChange={() => toggleAction(ct.name, action)} />
                                <label className="rs-mono" onClick={() => toggleAction(ct.name, action)}>{action}</label>
                                <span className="rs-perm-scope-key">content:{actionToScope(action)}</span>
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  );
                })}
                {cts.length === 0 && !contentTypes.loading && (
                  <div className="rs-perm-type-body" style={{ padding: "12px 14px" }}>
                    <span className="rs-cell-muted" style={{ fontSize: 12 }}>No collection types yet.</span>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

        <aside className="rs-editor-rail">
          <div className="rs-rail-card">
            <div className="rs-rail-card-head"><Icons.lock size={15} /> Token</div>
            <div className="rs-rail-field">
              <label>Type</label>
              <span className="rs-type-pill" style={{ alignSelf: "flex-start" }}>{presetMeta.name}</span>
            </div>
            <div className="rs-rail-divider" />
            <div className="rs-rail-stat"><span>Permissions</span><strong>{totalEnabled} / {totalActions}</strong></div>
            <div className="rs-rail-stat">
              <span>Expires</span>
              <strong>
                {token.expires_at
                  ? isExpired
                    ? <span className="rs-badge rs-badge--warn">Expired</span>
                    : fmtExpiry(token.expires_at)
                  : "Never"}
              </strong>
            </div>
            <div className="rs-rail-stat"><span>Last used</span><strong>{token.last_used_at ? relTime(token.last_used_at) : "—"}</strong></div>
            <div className="rs-rail-stat"><span>Created</span><strong>{relTime(token.created_at)}</strong></div>
            <div className="rs-rail-stat"><span>Status</span><strong>{isExpired ? "Expired" : "Active"}</strong></div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head"><Icons.bolt size={15} /> Save &amp; regenerate</div>
            <p className="rs-rail-note">
              Saving applies your changes by revoking the current token and issuing a new secret.
              Copy the new secret immediately — it is only shown once.
            </p>
          </div>
        </aside>
      </div>

      {revokeTarget && (
        <RevokeModal
          token={revokeTarget}
          onRevoked={() => navigate("/settings/api-tokens")}
          onClose={() => setRevokeTarget(null)}
        />
      )}
    </div>
  );
}

// ─── Revoke modal ─────────────────────────────────────────────────────────────

function RevokeModal({ token, onRevoked, onClose }: { token: ApiToken; onRevoked: () => void; onClose: () => void }) {
  const [revoking, setRevoking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    setRevoking(true);
    try {
      await revokeApiToken(token.id);
      onRevoked();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to revoke token.");
      setRevoking(false);
    }
  };

  return (
    <div className="rs-modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="rs-modal">
        <div className="rs-modal-head">
          <h2>Revoke token</h2>
          <button className="rs-modal-close" onClick={onClose}><Icons.x size={18} /></button>
        </div>
        <div className="rs-modal-body">
          {error && <Notice>{error}</Notice>}
          <p>Revoke <strong>{token.name}</strong>? Any client using this token will lose access immediately. This cannot be undone.</p>
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <button className="rs-btn rs-btn--primary" onClick={confirm} disabled={revoking}>
            {revoking ? "Revoking…" : "Revoke"}
          </button>
        </div>
      </div>
    </div>
  );
}
