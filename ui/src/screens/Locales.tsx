import { useState } from "react";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState, Notice } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { ApiError } from "../api/client";
import { listLocales, upsertLocale, deleteLocale, type Locale } from "../api/locales";

export function Locales() {
  const locales = useResource(() => listLocales(), []);
  const [adding, setAdding] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  if (locales.loading) return <LoadingState />;
  if (locales.error)
    return (
      <EmptyState>
        {locales.error.message}{" "}
        <button className="rs-link-btn" onClick={locales.refetch}>
          Retry
        </button>
      </EmptyState>
    );

  const rows = locales.data ?? [];

  const onDelete = async (code: string) => {
    setNotice(null);
    try {
      await deleteLocale(code);
      locales.refetch();
    } catch (e) {
      setNotice(e instanceof ApiError ? e.message : "Couldn't delete locale.");
    }
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Locales</h1>
          <p className="rs-cm-sub">
            {rows.length} locale{rows.length === 1 ? "" : "s"} configured
          </p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => setAdding(true)}>
          <Icons.plus size={16} /> Add locale
        </button>
      </div>

      {notice && (
        <div style={{ margin: "0 24px" }}>
          <Notice>{notice}</Notice>
        </div>
      )}

      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th>Code</th>
              <th>Name</th>
              <th>Default</th>
              <th className="rs-col-act" />
            </tr>
          </thead>
          <tbody>
            {rows.map((l) => (
              <tr key={l.code}>
                <td className="rs-mono">{l.code}</td>
                <td>{l.name}</td>
                <td>
                  {l.is_default ? (
                    <span className="rs-status rs-status--ok">Default</span>
                  ) : (
                    <span className="rs-cell-muted">—</span>
                  )}
                </td>
                <td className="rs-col-act">
                  {!l.is_default && (
                    <button
                      className="rs-row-btn rs-danger"
                      title="Delete locale"
                      onClick={() => onDelete(l.code)}
                    >
                      <Icons.trash size={15} />
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 0 && <div className="rs-empty">No locales configured.</div>}
      </div>

      {adding && (
        <AddLocaleModal
          onClose={() => setAdding(false)}
          onSaved={() => {
            setAdding(false);
            locales.refetch();
          }}
        />
      )}
    </div>
  );
}

function AddLocaleModal({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: (l: Locale) => void;
}) {
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [isDefault, setIsDefault] = useState(false);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const submit = async () => {
    setSaving(true);
    setErr(null);
    try {
      const l = await upsertLocale({
        code: code.trim(),
        name: name.trim(),
        is_default: isDefault,
      });
      onSaved(l);
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : "Couldn't add locale.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div
        className="rs-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className="rs-modal-head">
          <h2>Add locale</h2>
        </div>
        <div className="rs-modal-body">
          <div className="rs-field">
            <div className="rs-field-label">
              <label>Code</label>
              <span className="rs-field-hint">lowercase, e.g. fr or pt-br</span>
            </div>
            <input
              className="rs-input rs-mono"
              autoFocus
              placeholder="fr"
              value={code}
              onChange={(e) => setCode(e.target.value)}
            />
          </div>
          <div className="rs-field">
            <div className="rs-field-label">
              <label>Name</label>
            </div>
            <input
              className="rs-input"
              placeholder="French"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="rs-field">
            <div className="rs-field-label">
              <label>Default</label>
              <span className="rs-field-hint">Set as the default locale</span>
            </div>
            <button
              type="button"
              role="switch"
              aria-label="Set as default locale"
              aria-checked={isDefault}
              className={"rs-toggle" + (isDefault ? " is-on" : "")}
              onClick={() => setIsDefault((v) => !v)}
            >
              <span className="rs-toggle-knob" />
            </button>
          </div>
          {err && <div className="rs-err-msg">{err}</div>}
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose} disabled={saving}>
            Cancel
          </button>
          <button
            className="rs-btn rs-btn--primary"
            onClick={submit}
            disabled={saving || !code.trim() || !name.trim()}
          >
            {saving ? "Adding…" : "Add locale"}
          </button>
        </div>
      </div>
    </div>
  );
}
