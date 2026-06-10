import { useState } from "react";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState, Notice } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listApiTokens, createApiToken, revokeApiToken } from "../api/endpoints";
import type { ApiToken, NewApiToken } from "../api/types";
import { relTime } from "../util";
import { ApiError } from "../api/client";

const ALL_SCOPES = [
  { key: "content:read",  label: "Content — Read" },
  { key: "content:write", label: "Content — Write" },
  { key: "schema:read",   label: "Schema — Read" },
  { key: "schema:write",  label: "Schema — Write" },
  { key: "user:read",     label: "Users — Read" },
  { key: "user:write",    label: "Users — Write" },
];

export function ApiTokens() {
  const tokens = useResource(() => listApiTokens(), []);
  const [creating, setCreating] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<ApiToken | null>(null);
  const [revealed, setRevealed] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const copy = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>API Tokens</h1>
          <p className="rs-cm-sub">{(tokens.data ?? []).length} token{(tokens.data ?? []).length === 1 ? "" : "s"}</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => setCreating(true)}>
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
                <th>Scopes</th>
                <th>Expires</th>
                <th>Last used</th>
                <th>Created</th>
                <th className="rs-col-act" />
              </tr>
            </thead>
            <tbody>
              {(tokens.data ?? []).map((t) => (
                <tr key={t.id}>
                  <td className="rs-cell-title"><span className="rs-title-text">{t.name}</span></td>
                  <td>
                    <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
                      {t.scopes.map((s) => <span key={s} className="rs-type-pill">{s}</span>)}
                    </div>
                  </td>
                  <td className="rs-cell-muted">
                    {t.expires_at
                      ? new Date(t.expires_at) < new Date()
                        ? <span className="rs-badge rs-badge--warn">Expired</span>
                        : relTime(t.expires_at)
                      : "Never"}
                  </td>
                  <td className="rs-cell-muted">{t.last_used_at ? relTime(t.last_used_at) : "—"}</td>
                  <td className="rs-cell-muted">{relTime(t.created_at)}</td>
                  <td className="rs-col-act">
                    <button className="rs-row-btn rs-danger" title="Revoke token" onClick={() => setRevokeTarget(t)}>
                      <Icons.trash size={16} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {(tokens.data ?? []).length === 0 && <div className="rs-empty">No API tokens yet.</div>}
        </div>
      )}

      {creating && (
        <CreateModal
          revealed={revealed}
          copied={copied}
          onCopy={copy}
          onCreated={(raw) => { setRevealed(raw); tokens.refetch(); }}
          onClose={() => { setCreating(false); setRevealed(null); setCopied(false); }}
        />
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

function CreateModal({ revealed, copied, onCopy, onCreated, onClose }: {
  revealed: string | null; copied: boolean; onCopy: (t: string) => void;
  onCreated: (raw: string) => void; onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState<string[]>([]);
  const [expires, setExpires] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleScope = (s: string) =>
    setScopes((prev) => prev.includes(s) ? prev.filter((x) => x !== s) : [...prev, s]);

  const submit = async () => {
    setError(null);
    if (!name.trim()) { setError("Name is required."); return; }
    if (scopes.length === 0) { setError("Select at least one scope."); return; }
    setSaving(true);
    try {
      const body: NewApiToken = { name: name.trim(), scopes };
      if (expires) body.expires_at = new Date(expires).toISOString();
      const result = await createApiToken(body);
      onCreated(result.token);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to create token.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="rs-modal">
        <div className="rs-modal-head">
          <h2>{revealed ? "Token created" : "Create API token"}</h2>
          <button className="rs-modal-close" onClick={onClose}><Icons.x size={18} /></button>
        </div>
        <div className="rs-modal-body">
          {revealed ? (
            <>
              <Notice>Copy this token now — it won't be shown again.</Notice>
              <div className="rs-token-reveal">
                <code className="rs-mono">{revealed}</code>
                <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => onCopy(revealed)}>
                  {copied ? "Copied!" : <><Icons.copy size={14} /> Copy</>}
                </button>
              </div>
            </>
          ) : (
            <>
              {error && <Notice>{error}</Notice>}
              <div className="rs-field-row">
                <label className="rs-label">Name</label>
                <input className="rs-input" value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Website frontend" />
              </div>
              <div className="rs-field-row">
                <label className="rs-label">Scopes</label>
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {ALL_SCOPES.map((s) => (
                    <label key={s.key} style={{ display: "flex", alignItems: "center", gap: 8, cursor: "pointer" }}>
                      <input type="checkbox" checked={scopes.includes(s.key)} onChange={() => toggleScope(s.key)} />
                      <span>{s.label}</span>
                    </label>
                  ))}
                </div>
              </div>
              <div className="rs-field-row">
                <label className="rs-label">Expires (optional)</label>
                <input className="rs-input" type="date" value={expires} onChange={(e) => setExpires(e.target.value)} />
              </div>
            </>
          )}
        </div>
        <div className="rs-modal-foot">
          {revealed ? (
            <button className="rs-btn rs-btn--primary" onClick={onClose}>Done</button>
          ) : (
            <>
              <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
              <button className="rs-btn rs-btn--primary" onClick={submit} disabled={saving}>
                {saving ? "Creating…" : "Create token"}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

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
          <button className="rs-btn rs-btn--primary rs-danger" onClick={confirm} disabled={revoking}>
            {revoking ? "Revoking…" : "Revoke"}
          </button>
        </div>
      </div>
    </div>
  );
}
