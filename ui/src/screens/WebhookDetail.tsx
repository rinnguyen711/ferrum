import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import {
  listWebhooks,
  listDeliveries,
  testWebhook,
  setWebhookEnabled,
} from "../api/webhooks";
import type { Webhook } from "../api/webhooks";
import { ApiError } from "../api/client";
import { relTime } from "../util";

// ─── Webhook detail ───────────────────────────────────────────────────────────

export function WebhookDetail() {
  const { id = "" } = useParams();
  const navigate = useNavigate();

  // No GET-single endpoint — pull the list and pick this one out.
  const webhooks = useResource(() => listWebhooks(), []);
  const deliveries = useResource(() => listDeliveries(id), [id]);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const back = () => navigate("/settings/webhooks");

  if (webhooks.loading) return <LoadingState />;
  if (webhooks.error) return <EmptyState>{webhooks.error.message}</EmptyState>;

  const webhook = (webhooks.data ?? []).find((w) => w.id === id);
  if (!webhook) {
    return (
      <div className="rs-editor">
        <div className="rs-editor-bar">
          <button className="rs-back" onClick={back}><Icons.arrowLeft size={18} /></button>
          <div className="rs-editor-titlewrap"><h1>Webhook not found</h1></div>
        </div>
        <div className="rs-editor-body">
          <div className="rs-editor-main">
            <EmptyState>This webhook no longer exists.</EmptyState>
          </div>
        </div>
      </div>
    );
  }

  const toggle = async (w: Webhook) => {
    setError(null);
    setBusy(true);
    try {
      await setWebhookEnabled(w, !w.enabled);
      webhooks.refetch();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to update webhook.");
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setError(null);
    try {
      await testWebhook(webhook.id);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Test failed.");
    }
  };

  return (
    <div className="rs-editor">
      {/* ── Top bar ── */}
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={back}><Icons.arrowLeft size={18} /></button>
        <div className="rs-editor-titlewrap">
          <h1>{webhook.name}</h1>
          <div className="rs-editor-meta">
            <span className="rs-method">POST</span>
            <span className="rs-cell-muted">· {webhook.events.length} {webhook.events.length === 1 ? "event" : "events"}</span>
          </div>
        </div>
        <div className="rs-editor-actions">
          <button
            className={"rs-toggle" + (webhook.enabled ? " is-on" : "")}
            disabled={busy}
            title={webhook.enabled ? "Enabled — click to disable" : "Disabled — click to enable"}
            onClick={() => toggle(webhook)}
          >
            <span className="rs-toggle-knob" />
          </button>
          <button className="rs-btn rs-btn--ghost" onClick={test}>
            <Icons.bolt size={15} /> Send test
          </button>
        </div>
      </div>

      {/* ── Body ── */}
      <div className="rs-editor-body">
        <div className="rs-editor-main">
          {error && (
            <div style={{
              padding: "8px 12px",
              background: "var(--danger-bg)",
              color: "var(--danger)",
              borderRadius: "var(--r-sm)",
              fontSize: 13,
              marginBottom: 18,
            }}>
              {error}
            </div>
          )}

          {/* Endpoint */}
          <div className="rs-field" style={{ marginBottom: "var(--field-gap)" }}>
            <div className="rs-field-label"><label>Endpoint</label></div>
            <div className="rs-input-affix">
              <span className="rs-affix rs-mono">POST</span>
              <input className="rs-input rs-mono" value={webhook.url} readOnly />
            </div>
          </div>

          {/* Events */}
          <div className="rs-field" style={{ marginBottom: "var(--field-gap)" }}>
            <div className="rs-field-label"><label>Events</label></div>
            <span style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
              {webhook.events.map((ev) => (
                <code key={ev} className="rs-mono rs-event-api">{ev}</code>
              ))}
            </span>
          </div>

          {/* Recent deliveries */}
          <div className="rs-field">
            <div className="rs-field-label">
              <label>Recent deliveries</label>
              <button className="rs-link-btn" onClick={() => deliveries.refetch()}>Refresh</button>
            </div>
            <DeliveryLog
              loading={deliveries.loading}
              items={(deliveries.data ?? []).slice(0, 20)}
            />
          </div>
        </div>

        {/* ── Rail ── */}
        <aside className="rs-editor-rail">
          <div className="rs-rail-card">
            <div className="rs-rail-card-head"><Icons.link size={15} /> Summary</div>
            <div className="rs-rail-stat"><span>Status</span><strong>{webhook.enabled ? "Enabled" : "Disabled"}</strong></div>
            <div className="rs-rail-stat"><span>Events</span><strong>{webhook.events.length}</strong></div>
            <div className="rs-rail-stat"><span>Created</span><strong>{relTime(webhook.created_at)}</strong></div>
          </div>
        </aside>
      </div>
    </div>
  );
}

// ─── Delivery log ─────────────────────────────────────────────────────────────

function DeliveryLog({
  loading,
  items,
}: {
  loading: boolean;
  items: { id: string; event: string; status: string; attempt: number; last_error: string | null; created_at: string }[];
}) {
  if (loading) {
    return <span className="rs-cell-muted" style={{ fontSize: 12.5 }}>Loading deliveries…</span>;
  }
  if (items.length === 0) {
    return <span className="rs-cell-muted" style={{ fontSize: 12.5 }}>No deliveries yet.</span>;
  }

  return (
    <div className="rs-table-wrap">
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
              <td className="rs-cell-muted" style={{ maxWidth: 280, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
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
      <span className="rs-status" style={{ color: "var(--danger)", background: "var(--danger-bg)" }}>
        {status}
      </span>
    );
  }
  return <span className="rs-status rs-status--muted">{status}</span>;
}
