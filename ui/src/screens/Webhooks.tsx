import React, { useState } from "react";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import {
  listWebhooks,
  createWebhook,
  deleteWebhook,
  testWebhook,
  listDeliveries,
  WEBHOOK_EVENTS,
} from "../api/webhooks";
import type { Webhook, WebhookDelivery } from "../api/webhooks";
import { ApiError } from "../api/client";
import { relTime } from "../util";

// ─── Flash notification ───────────────────────────────────────────────────────

interface Flash {
  id: number;
  kind: "ok" | "err";
  msg: string;
}

let _flashId = 0;

// ─── Webhooks screen ──────────────────────────────────────────────────────────

export function Webhooks() {
  const webhooks = useResource(() => listWebhooks(), []);
  const [showCreate, setShowCreate] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Webhook | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [flashes, setFlashes] = useState<Flash[]>([]);

  const addFlash = (kind: Flash["kind"], msg: string) => {
    const id = ++_flashId;
    setFlashes((f) => [...f, { id, kind, msg }]);
    setTimeout(() => setFlashes((f) => f.filter((x) => x.id !== id)), 4000);
  };

  const toggleRow = (id: string) => {
    setExpandedId((cur) => (cur === id ? null : id));
  };

  const handleTest = async (w: Webhook, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await testWebhook(w.id);
      addFlash("ok", `Test sent to "${w.name}".`);
    } catch (err) {
      addFlash("err", err instanceof ApiError ? err.message : "Test failed.");
    }
  };

  return (
    <div className="rs-cm">
      {/* Flash notifications */}
      {flashes.length > 0 && (
        <div style={{
          position: "fixed",
          bottom: 24,
          right: 24,
          display: "flex",
          flexDirection: "column",
          gap: 8,
          zIndex: 1000,
        }}>
          {flashes.map((f) => (
            <div
              key={f.id}
              style={{
                padding: "10px 16px",
                borderRadius: "var(--r-md)",
                background: f.kind === "ok" ? "var(--ok-bg)" : "var(--danger-bg)",
                color: f.kind === "ok" ? "var(--ok)" : "var(--danger)",
                border: `1px solid ${f.kind === "ok" ? "var(--ok)" : "var(--danger)"}`,
                fontSize: 13.5,
                boxShadow: "var(--shadow-md)",
              }}
            >
              {f.msg}
            </div>
          ))}
        </div>
      )}

      <div className="rs-cm-head">
        <div>
          <h1>Webhooks</h1>
          <p className="rs-cm-sub">
            {(webhooks.data ?? []).length} webhook{(webhooks.data ?? []).length === 1 ? "" : "s"}
          </p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => setShowCreate(true)}>
          <Icons.plus size={16} /> Add Webhook
        </button>
      </div>

      {webhooks.loading && <LoadingState />}
      {webhooks.error && <EmptyState>{webhooks.error.message}</EmptyState>}

      {!webhooks.loading && !webhooks.error && (
        <div className="rs-table-wrap">
          <table className="rs-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>URL</th>
                <th>Events</th>
                <th>Enabled</th>
                <th className="rs-col-act" />
              </tr>
            </thead>
            <tbody>
              {(webhooks.data ?? []).map((w) => (
                <React.Fragment key={w.id}>
                  <tr
                    className="rs-row-link"
                    style={{ cursor: "pointer" }}
                    onClick={() => toggleRow(w.id)}
                  >
                    <td className="rs-cell-title">
                      <span className="rs-title-text">{w.name}</span>
                    </td>
                    <td className="rs-cell-muted rs-mono" style={{ fontSize: 12.5 }}>
                      {w.url.length > 40 ? w.url.slice(0, 40) + "…" : w.url}
                    </td>
                    <td>
                      <span style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                        {w.events.map((ev) => (
                          <span
                            key={ev}
                            className="rs-type-pill"
                            style={{ fontSize: 11, padding: "1px 6px" }}
                          >
                            {ev}
                          </span>
                        ))}
                      </span>
                    </td>
                    <td>
                      {w.enabled
                        ? <span className="rs-status rs-status--ok">Yes</span>
                        : <span className="rs-status rs-status--muted">No</span>}
                    </td>
                    <td className="rs-col-act" style={{ display: "flex", gap: 4 }}>
                      <button
                        className="rs-row-btn"
                        title="Send test"
                        onClick={(e) => handleTest(w, e)}
                      >
                        <Icons.bolt size={15} />
                      </button>
                      <button
                        className="rs-row-btn rs-danger"
                        title="Delete webhook"
                        onClick={(e) => { e.stopPropagation(); setDeleteTarget(w); }}
                      >
                        <Icons.trash size={15} />
                      </button>
                    </td>
                  </tr>
                  {expandedId === w.id && (
                    <tr>
                      <td colSpan={5} style={{ padding: 0, background: "var(--surface-2)" }}>
                        <DeliveryLog webhookId={w.id} />
                      </td>
                    </tr>
                  )}
                </React.Fragment>
              ))}
            </tbody>
          </table>
          {(webhooks.data ?? []).length === 0 && (
            <div className="rs-empty">No webhooks yet. Add one to receive event notifications.</div>
          )}
        </div>
      )}

      {showCreate && (
        <CreateModal
          onCreated={() => { setShowCreate(false); webhooks.refetch(); addFlash("ok", "Webhook created."); }}
          onClose={() => setShowCreate(false)}
          onError={(msg) => addFlash("err", msg)}
        />
      )}

      {deleteTarget && (
        <DeleteModal
          webhook={deleteTarget}
          onDeleted={() => { setDeleteTarget(null); webhooks.refetch(); addFlash("ok", `"${deleteTarget.name}" deleted.`); }}
          onClose={() => setDeleteTarget(null)}
          onError={(msg) => addFlash("err", msg)}
        />
      )}
    </div>
  );
}

// ─── Delivery log ─────────────────────────────────────────────────────────────

function DeliveryLog({ webhookId }: { webhookId: string }) {
  const deliveries = useResource(() => listDeliveries(webhookId), [webhookId]);

  if (deliveries.loading) {
    return (
      <div style={{ padding: "12px 20px" }}>
        <span className="rs-cell-muted" style={{ fontSize: 12 }}>Loading deliveries…</span>
      </div>
    );
  }

  const items = (deliveries.data ?? []).slice(0, 10);

  if (items.length === 0) {
    return (
      <div style={{ padding: "12px 20px" }}>
        <span className="rs-cell-muted" style={{ fontSize: 12 }}>No deliveries yet.</span>
      </div>
    );
  }

  return (
    <div style={{ padding: "8px 20px 12px" }}>
      <p style={{ fontSize: 11.5, fontWeight: 600, letterSpacing: ".05em", textTransform: "uppercase", color: "var(--text-subtle)", marginBottom: 8 }}>
        Recent deliveries
      </p>
      <table className="rs-table" style={{ fontSize: 12.5 }}>
        <thead>
          <tr>
            <th>Event</th>
            <th>Status</th>
            <th>Attempt</th>
            <th>Error</th>
            <th>Time</th>
          </tr>
        </thead>
        <tbody>
          {items.map((d) => (
            <tr key={d.id}>
              <td className="rs-mono">{d.event}</td>
              <td><DeliveryStatus status={d.status} /></td>
              <td style={{ fontVariantNumeric: "tabular-nums" }}>{d.attempt}</td>
              <td className="rs-cell-muted" style={{ maxWidth: 240, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {d.last_error ?? "—"}
              </td>
              <td className="rs-cell-muted">{relTime(d.created_at)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function DeliveryStatus({ status }: { status: string }) {
  if (status === "success") {
    return <span className="rs-status rs-status--ok">{status}</span>;
  }
  if (status === "failed") {
    return (
      <span
        className="rs-status"
        style={{ color: "var(--danger)", background: "var(--danger-bg)" }}
      >
        {status}
      </span>
    );
  }
  return <span className="rs-status rs-status--muted">{status}</span>;
}

// ─── Create modal ─────────────────────────────────────────────────────────────

function CreateModal({
  onCreated,
  onClose,
  onError,
}: {
  onCreated: () => void;
  onClose: () => void;
  onError: (msg: string) => void;
}) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [events, setEvents] = useState<Set<string>>(new Set());
  const [secret, setSecret] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleEvent = (ev: string) => {
    setEvents((prev) => {
      const next = new Set(prev);
      next.has(ev) ? next.delete(ev) : next.add(ev);
      return next;
    });
  };

  const valid = name.trim().length > 0 && url.trim().length > 0 && events.size > 0;

  const submit = async () => {
    setError(null);
    setSaving(true);
    try {
      await createWebhook({
        name: name.trim(),
        url: url.trim(),
        events: Array.from(events),
        secret: secret.trim() || undefined,
      });
      onCreated();
    } catch (e) {
      const msg = e instanceof ApiError ? e.message : "Failed to create webhook.";
      setError(msg);
      onError(msg);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="rs-modal">
        <div className="rs-modal-head">
          <h2>Add Webhook</h2>
          <button className="rs-modal-close" onClick={onClose}><Icons.x size={18} /></button>
        </div>
        <div className="rs-modal-body" style={{ display: "flex", flexDirection: "column", gap: "var(--field-gap)" }}>
          {error && (
            <div style={{
              padding: "8px 12px",
              background: "var(--danger-bg)",
              color: "var(--danger)",
              borderRadius: "var(--r-sm)",
              fontSize: 13,
            }}>
              {error}
            </div>
          )}

          <div className="rs-field">
            <label className="rs-field-label">
              Name <span className="rs-req">*</span>
            </label>
            <input
              className="rs-input"
              value={name}
              placeholder="e.g. Slack notifications"
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          <div className="rs-field">
            <label className="rs-field-label">
              URL <span className="rs-req">*</span>
            </label>
            <input
              className="rs-input"
              type="url"
              value={url}
              placeholder="https://example.com/hooks/rustapi"
              onChange={(e) => setUrl(e.target.value)}
            />
          </div>

          <div className="rs-field">
            <label className="rs-field-label">
              Events <span className="rs-req">*</span>
            </label>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              {WEBHOOK_EVENTS.map((ev) => (
                <label
                  key={ev}
                  style={{ display: "flex", alignItems: "center", gap: 8, cursor: "pointer", fontSize: 13.5 }}
                >
                  <span
                    className={"rs-check" + (events.has(ev) ? " is-on" : "")}
                    onClick={() => toggleEvent(ev)}
                    style={{ flexShrink: 0 }}
                  />
                  <span className="rs-mono" style={{ fontSize: 12.5 }}>{ev}</span>
                </label>
              ))}
            </div>
          </div>

          <div className="rs-field">
            <label className="rs-field-label">Secret</label>
            <input
              className="rs-input"
              type="password"
              value={secret}
              placeholder="Leave blank to skip signing"
              onChange={(e) => setSecret(e.target.value)}
            />
          </div>
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <button
            className="rs-btn rs-btn--primary"
            disabled={!valid || saving}
            onClick={submit}
          >
            {saving ? "Creating…" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Delete modal ─────────────────────────────────────────────────────────────

function DeleteModal({
  webhook,
  onDeleted,
  onClose,
  onError,
}: {
  webhook: Webhook;
  onDeleted: () => void;
  onClose: () => void;
  onError: (msg: string) => void;
}) {
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    setDeleting(true);
    try {
      await deleteWebhook(webhook.id);
      onDeleted();
    } catch (e) {
      const msg = e instanceof ApiError ? e.message : "Failed to delete webhook.";
      setError(msg);
      onError(msg);
      setDeleting(false);
    }
  };

  return (
    <div className="rs-modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="rs-modal">
        <div className="rs-modal-head">
          <h2>Delete webhook</h2>
          <button className="rs-modal-close" onClick={onClose}><Icons.x size={18} /></button>
        </div>
        <div className="rs-modal-body">
          {error && (
            <div style={{
              padding: "8px 12px",
              background: "var(--danger-bg)",
              color: "var(--danger)",
              borderRadius: "var(--r-sm)",
              fontSize: 13,
              marginBottom: 12,
            }}>
              {error}
            </div>
          )}
          <p>
            Delete <strong>{webhook.name}</strong>? Future events will no longer be delivered to this endpoint. This cannot be undone.
          </p>
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <button
            className="rs-btn rs-btn--primary"
            style={{ background: "var(--danger)", borderColor: "var(--danger)" }}
            disabled={deleting}
            onClick={confirm}
          >
            {deleting ? "Deleting…" : "Delete"}
          </button>
        </div>
      </div>
    </div>
  );
}
