import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { createWebhook } from "../api/webhooks";
import { ApiError } from "../api/client";

// ─── Event metadata ────────────────────────────────────────────────────────────

const WH_EVENTS = [
  { key: "entry.created",     label: "Create",     desc: "Fires when a new entry is created." },
  { key: "entry.updated",     label: "Update",     desc: "Fires when an existing entry is saved." },
  { key: "entry.deleted",     label: "Delete",     desc: "Fires when an entry is removed." },
  { key: "entry.published",   label: "Publish",    desc: "Fires when a draft is published live." },
  { key: "entry.unpublished", label: "Unpublish",  desc: "Fires when a published entry is unpublished." },
] as const;

// ─── Header row ────────────────────────────────────────────────────────────────

interface HeaderRow {
  id: string;
  key: string;
  value: string;
}

let _uid = 0;
function newHeader(key = "", value = ""): HeaderRow {
  return { id: `h${++_uid}`, key, value };
}

// ─── WebhookEditor ────────────────────────────────────────────────────────────

export function WebhookEditor() {
  const navigate = useNavigate();

  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [headers, setHeaders] = useState<HeaderRow[]>([
    newHeader("Content-Type", "application/json"),
  ]);
  const [events, setEvents] = useState<Set<string>>(new Set());
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const touch = () => setDirty(true);

  const setHeader = (id: string, patch: Partial<HeaderRow>) => {
    setHeaders((hs) => hs.map((h) => (h.id === id ? { ...h, ...patch } : h)));
    touch();
  };
  const addHeader = () => { setHeaders((hs) => [...hs, newHeader()]); touch(); };
  const removeHeader = (id: string) => { setHeaders((hs) => hs.filter((h) => h.id !== id)); touch(); };

  const toggleEvent = (k: string) => {
    setEvents((s) => { const n = new Set(s); n.has(k) ? n.delete(k) : n.add(k); return n; });
    touch();
  };
  const allOn = events.size === WH_EVENTS.length;
  const toggleAll = () => {
    setEvents(allOn ? new Set() : new Set(WH_EVENTS.map((e) => e.key)));
    touch();
  };

  const activeHeaders = headers.filter((h) => h.key.trim());
  const valid = name.trim().length > 0 && /^https?:\/\/.+/.test(url.trim()) && events.size > 0;

  const submit = async () => {
    setError(null);
    setSaving(true);
    try {
      await createWebhook({
        name: name.trim(),
        url: url.trim(),
        events: Array.from(events),
        headers: activeHeaders.map(({ key, value }) => ({ key, value })),
      });
      navigate("/settings/webhooks", { replace: true });
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to create webhook.");
      setSaving(false);
    }
  };

  return (
    <div className="rs-editor">
      {/* ── Top bar ── */}
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={() => navigate("/settings/webhooks")}>
          <Icons.arrowLeft size={18} />
        </button>
        <div className="rs-editor-titlewrap">
          <h1>Create webhook</h1>
          <div className="rs-editor-meta">
            <span className="rs-method">POST</span>
            <span className="rs-cell-muted">
              · {events.size} {events.size === 1 ? "event" : "events"}
            </span>
            {dirty && (
              <span className="rs-unsaved">
                <span className="rs-dot" /> Unsaved
              </span>
            )}
          </div>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" onClick={() => navigate("/settings/webhooks")}>
            Cancel
          </button>
          <button className="rs-btn rs-btn--primary" disabled={!valid || saving} onClick={submit}>
            <Icons.check size={15} />
            {saving ? "Creating…" : "Create webhook"}
          </button>
        </div>
      </div>

      {/* ── Body ── */}
      <div className="rs-editor-body">
        {/* ── Main ── */}
        <div className="rs-editor-main">
          {error && (
            <div style={{
              padding: "8px 12px",
              background: "var(--danger-bg)",
              color: "var(--danger)",
              borderRadius: "var(--r-sm)",
              fontSize: 13,
              marginBottom: "var(--field-gap)",
            }}>
              {error}
            </div>
          )}

          <div className="rs-fields rs-fields--single">
            {/* Name */}
            <div className="rs-field">
              <div className="rs-field-label">
                <label>Name <span className="rs-req">*</span></label>
                <span className="rs-field-hint">Shown in the webhooks list</span>
              </div>
              <input
                className="rs-input"
                value={name}
                placeholder="e.g. Vercel production deploy"
                onChange={(e) => { setName(e.target.value); touch(); }}
              />
            </div>

            {/* URL */}
            <div className="rs-field">
              <div className="rs-field-label">
                <label>URL <span className="rs-req">*</span></label>
                <span className="rs-field-hint">The endpoint Ferrum sends the POST request to</span>
              </div>
              <div className="rs-input-affix">
                <span className="rs-affix rs-mono">POST</span>
                <input
                  className="rs-input rs-mono"
                  value={url}
                  placeholder="https://example.com/webhooks/ferrum"
                  onChange={(e) => { setUrl(e.target.value); touch(); }}
                />
              </div>
            </div>

            {/* Headers */}
            <div className="rs-field">
              <div className="rs-field-label">
                <label>Headers</label>
                <span className="rs-field-hint">
                  Sent with every request — use for auth tokens or content negotiation
                </span>
              </div>
              <div className="rs-kv">
                <div className="rs-kv-head">
                  <span>Key</span>
                  <span>Value</span>
                  <span />
                </div>
                {headers.length === 0 && (
                  <div className="rs-kv-empty">No custom headers.</div>
                )}
                {headers.map((h) => (
                  <div className="rs-kv-row" key={h.id}>
                    <input
                      className="rs-input rs-input--sm rs-mono"
                      value={h.key}
                      placeholder="Authorization"
                      onChange={(e) => setHeader(h.id, { key: e.target.value })}
                    />
                    <input
                      className="rs-input rs-input--sm rs-mono"
                      value={h.value}
                      placeholder="Bearer •••••"
                      onChange={(e) => setHeader(h.id, { value: e.target.value })}
                    />
                    <button className="rs-row-btn rs-danger" title="Remove header" onClick={() => removeHeader(h.id)}>
                      <Icons.trash size={15} />
                    </button>
                  </div>
                ))}
                <button className="rs-kv-add" onClick={addHeader}>
                  <Icons.plus size={15} /> Add header
                </button>
              </div>
            </div>

            {/* Events */}
            <div className="rs-field">
              <div className="rs-field-label">
                <label>Events <span className="rs-req">*</span></label>
                <span className="rs-field-hint">
                  {events.size} of {WH_EVENTS.length} entry events selected
                </span>
              </div>
              <div className="rs-events">
                <div className="rs-events-head">
                  <span>Entry lifecycle</span>
                  <button className="rs-link-btn" onClick={toggleAll}>
                    {allOn ? "Clear all" : "Select all"}
                  </button>
                </div>
                {WH_EVENTS.map((ev) => {
                  const on = events.has(ev.key);
                  return (
                    <label className={"rs-event-row" + (on ? " is-on" : "")} key={ev.key} onClick={() => toggleEvent(ev.key)}>
                      <button
                        type="button"
                        className={"rs-check" + (on ? " is-on" : "")}
                        role="checkbox"
                        aria-checked={on}
                        onClick={(e) => e.preventDefault()}
                      >
                        {on && <Icons.check size={13} />}
                      </button>
                      <div className="rs-event-meta">
                        <strong>{ev.label}</strong>
                        <span className="rs-cell-muted">{ev.desc}</span>
                      </div>
                      <code className="rs-mono rs-event-api">{ev.key}</code>
                    </label>
                  );
                })}
              </div>
            </div>
          </div>
        </div>

        {/* ── Rail ── */}
        <aside className="rs-editor-rail">
          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.link size={15} /> Summary
            </div>
            <div className="rs-rail-field">
              <label>Method</label>
              <span className="rs-method" style={{ alignSelf: "flex-start" }}>POST</span>
            </div>
            <div className="rs-rail-divider" />
            <div className="rs-rail-stat">
              <span>Events</span>
              <strong>{events.size} / {WH_EVENTS.length}</strong>
            </div>
            <div className="rs-rail-stat">
              <span>Headers</span>
              <strong>{activeHeaders.length}</strong>
            </div>
            <div className="rs-rail-stat">
              <span>Status</span>
              <strong>Draft</strong>
            </div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.braces size={15} /> Example payload
            </div>
            <p className="rs-rail-note">
              Each enabled event delivers a signed JSON body to your endpoint:
            </p>
            <pre className="rs-code rs-code--rail">
              <code>{JSON.stringify({
                event: "entry.publish",
                createdAt: "2026-06-10T09:14:00Z",
                model: "article",
                entry: { id: 142, title: "…", status: "published" },
              }, null, 2)}</code>
            </pre>
          </div>
        </aside>
      </div>
    </div>
  );
}
