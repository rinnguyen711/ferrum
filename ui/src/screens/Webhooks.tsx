import React, { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import {
  listWebhooks,
  deleteWebhook,
  testWebhook,
  setWebhookEnabled,
} from "../api/webhooks";
import type { Webhook } from "../api/webhooks";
import { ApiError } from "../api/client";

// ─── Flash notification ───────────────────────────────────────────────────────

interface Flash {
  id: number;
  kind: "ok" | "err";
  msg: string;
}

let _flashId = 0;

// ─── Webhooks screen ──────────────────────────────────────────────────────────

export function Webhooks() {
  const navigate = useNavigate();
  const webhooks = useResource(() => listWebhooks(), []);
  const [deleteTarget, setDeleteTarget] = useState<Webhook | null>(null);
  const [togglingId, setTogglingId] = useState<string | null>(null);
  const [flashes, setFlashes] = useState<Flash[]>([]);

  const addFlash = (kind: Flash["kind"], msg: string) => {
    const id = ++_flashId;
    setFlashes((f) => [...f, { id, kind, msg }]);
    setTimeout(() => setFlashes((f) => f.filter((x) => x.id !== id)), 4000);
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

  const handleToggle = async (w: Webhook, e: React.MouseEvent) => {
    e.stopPropagation();
    setTogglingId(w.id);
    try {
      await setWebhookEnabled(w, !w.enabled);
      webhooks.refetch();
    } catch (err) {
      addFlash("err", err instanceof ApiError ? err.message : "Failed to update webhook.");
    } finally {
      setTogglingId(null);
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
        <button className="rs-btn rs-btn--primary" onClick={() => navigate("/settings/webhooks/new")}>
          <Icons.plus size={16} /> Create webhook
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
                <tr
                  key={w.id}
                  className="rs-row-link"
                  style={{ cursor: "pointer" }}
                  onClick={() => navigate(`/settings/webhooks/${w.id}`)}
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
                  <td onClick={(e) => e.stopPropagation()}>
                    <button
                      className={"rs-toggle" + (w.enabled ? " is-on" : "")}
                      disabled={togglingId === w.id}
                      title={w.enabled ? "Enabled — click to disable" : "Disabled — click to enable"}
                      onClick={(e) => handleToggle(w, e)}
                    >
                      <span className="rs-toggle-knob" />
                    </button>
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
              ))}
            </tbody>
          </table>
          {(webhooks.data ?? []).length === 0 && (
            <div className="rs-empty">No webhooks yet. Add one to receive event notifications.</div>
          )}
        </div>
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
