import { useEffect, useId, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { Notice, EditorBar, ConfirmDialog, LoadingState, EmptyState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listUsers, listRoles, createUser, updateUser, deleteUser } from "../api/endpoints";
import { ApiError } from "../api/client";
import { initials, relTime, AVATAR_NEUTRAL } from "../util";

type Tab = "account" | "permissions" | "api";

export function UserEditor() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const isNew = !id || id === "new";

  // For edit, load the user from the list (no single-get endpoint this slice).
  const users = useResource(() => listUsers(), []);
  const roles_ = useResource(() => listRoles(), []);
  const rolesData = roles_.data ?? [];
  const existing = isNew ? null : (users.data ?? []).find((u) => u.id === id) ?? null;

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [roles, setRoles] = useState<string[]>(["editor"]);
  const [confirmed, setConfirmed] = useState(true);
  const [blocked, setBlocked] = useState(false);
  const [tab, setTab] = useState<Tab>("account");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmDel, setConfirmDel] = useState(false);

  // Hydrate form once the user loads (edit mode). Keyed on the resolved user so
  // it runs after the list fetch settles, not during render.
  const hydrated = useRef(false);
  useEffect(() => {
    if (isNew || !existing || hydrated.current) return;
    setEmail(existing.email);
    setRoles(existing.roles.length ? existing.roles : ["editor"]);
    setConfirmed(existing.confirmed);
    setBlocked(existing.blocked);
    hydrated.current = true;
  }, [isNew, existing]);

  const tabsId = useId();

  const toggleRole = (key: string) => {
    setRoles((rs) => (rs.includes(key) ? rs.filter((r) => r !== key) : [...rs, key]));
  };

  const roleOf = (key: string) => rolesData.find((r) => r.key === key);
  const primaryRole = roles[0] ? roleOf(roles[0]) : undefined;

  const status: { cls: string; label: string } = blocked
    ? { cls: "rs-status--muted", label: "Blocked" }
    : confirmed
      ? { cls: "rs-status--ok", label: "Active" }
      : { cls: "rs-status--warn", label: "Pending" };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      if (isNew) {
        await createUser({ email, password, roles });
      } else {
        const patch: {
          email?: string;
          password?: string;
          roles?: string[];
          confirmed?: boolean;
          blocked?: boolean;
        } = { email, roles, confirmed, blocked };
        if (password) patch.password = password;
        await updateUser(id!, patch);
      }
      navigate("/users");
    } catch (e) {
      if (e instanceof ApiError) {
        if (e.fieldErrors.length) setError(e.fieldErrors[0].message ?? e.message);
        else setError(e.message);
      } else setError("Something went wrong.");
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (isNew || !id) return;
    setBusy(true);
    setError(null);
    try {
      await deleteUser(id);
      navigate("/users");
    } catch (e) {
      if (e instanceof ApiError) setError(e.message);
      else setError("Something went wrong.");
      setConfirmDel(false);
    } finally {
      setBusy(false);
    }
  };

  const tabs: [Tab, string][] = [
    ["account", "Account"],
    ["permissions", "Role & permissions"],
    ["api", "API & preview"],
  ];

  // Reason the save button is disabled — surfaced to screen readers so the
  // greyed-out control isn't a dead end.
  const saveHint = !email
    ? "Enter an email address to save."
    : isNew && !password
      ? "Enter a password to create the user."
      : null;

  const moveTab = (dir: 1 | -1) => {
    const i = tabs.findIndex(([k]) => k === tab);
    const next = tabs[(i + dir + tabs.length) % tabs.length][0];
    setTab(next);
    document.getElementById(`${tabsId}-tab-${next}`)?.focus();
  };

  // No single-user GET this slice — edit hydrates from the list. Guard the two
  // states that produces: still fetching, and a deep-linked id not in the list.
  if (!isNew && (users.loading || roles_.loading)) {
    return (
      <div className="rs-editor">
        <EditorBar onBack={() => navigate("/users")} title="User" />
        <LoadingState />
      </div>
    );
  }
  if (!isNew && !existing) {
    return (
      <div className="rs-editor">
        <EditorBar onBack={() => navigate("/users")} title="User" />
        <EmptyState>
          User not found. It may have been deleted.{" "}
          <button className="rs-link-btn" onClick={() => navigate("/users")}>
            Back to users
          </button>
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="rs-editor">
      <EditorBar
        onBack={() => navigate("/users")}
        title={isNew ? "Add a user" : email || "User"}
        status={
          !isNew && (
            <div className="rs-editor-meta">
              <span className={"rs-status " + status.cls}>
                <span className="rs-dot" /> {status.label}
              </span>
              {id && <span className="rs-cell-muted">· User ID {id.slice(0, 8)}</span>}
            </div>
          )
        }
        actions={
          <>
            {!isNew && (
              <button
                className="rs-btn rs-btn--ghost rs-danger"
                disabled={busy}
                onClick={() => setConfirmDel(true)}
              >
                <Icons.trash size={15} /> Delete
              </button>
            )}
            <button
              className="rs-btn rs-btn--primary"
              disabled={busy || !email || (isNew && !password)}
              aria-describedby={saveHint ? `${tabsId}-save-hint` : undefined}
              onClick={save}
            >
              <Icons.bolt size={15} /> {isNew ? "Create user" : "Save user"}
            </button>
            {saveHint && (
              <span id={`${tabsId}-save-hint`} className="rs-sr-only">
                {saveHint}
              </span>
            )}
          </>
        }
      />

      <div role="alert" aria-live="assertive">
        {error && (
          <div style={{ margin: "0 24px" }}>
            <Notice>{error}</Notice>
          </div>
        )}
      </div>

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-editor-tabs" role="tablist" aria-label="User settings">
            {tabs.map(([k, l]) => (
              <button
                key={k}
                id={`${tabsId}-tab-${k}`}
                role="tab"
                aria-selected={tab === k}
                aria-controls={`${tabsId}-panel-${k}`}
                tabIndex={tab === k ? 0 : -1}
                className={"rs-etab" + (tab === k ? " is-active" : "")}
                onClick={() => setTab(k)}
                onKeyDown={(e) => {
                  if (e.key === "ArrowRight") moveTab(1);
                  else if (e.key === "ArrowLeft") moveTab(-1);
                }}
              >
                {l}
              </button>
            ))}
          </div>

          {tab === "account" && (
            <div
              className="rs-fields"
              role="tabpanel"
              id={`${tabsId}-panel-account`}
              aria-labelledby={`${tabsId}-tab-account`}
            >
              <label className="rs-field rs-field--full">
                <span className="rs-field-label">
                  Email <span className="rs-req">*</span>
                  <span className="rs-field-hint">Sign-in address</span>
                </span>
                <input
                  className="rs-input"
                  type="email"
                  value={email}
                  placeholder="name@company.com"
                  onChange={(e) => setEmail(e.target.value)}
                />
              </label>

              <label className="rs-field rs-field--full">
                <span className="rs-field-label">
                  {isNew ? "Password" : "Reset password"}
                  {isNew && <span className="rs-req">*</span>}
                  <span className="rs-field-hint">
                    {isNew ? "At least 8 characters" : "Leave blank to keep current"}
                  </span>
                </span>
                <input
                  className="rs-input"
                  type="password"
                  value={password}
                  placeholder={isNew ? "At least 8 characters" : "Leave blank to keep current"}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </label>

              <div className="rs-field">
                <span className="rs-field-label">
                  Confirmed
                  <span className="rs-field-hint">Has the user verified their email?</span>
                </span>
                <button
                  type="button"
                  role="switch"
                  aria-label="Confirmed"
                  aria-checked={confirmed}
                  className={"rs-toggle" + (confirmed ? " is-on" : "")}
                  onClick={() => setConfirmed((v) => !v)}
                >
                  <span className="rs-toggle-knob" />
                </button>
              </div>

              <div className="rs-field">
                <span className="rs-field-label">
                  Blocked
                  <span className="rs-field-hint">Blocked users cannot sign in</span>
                </span>
                <button
                  type="button"
                  role="switch"
                  aria-label="Blocked"
                  aria-checked={blocked}
                  className={"rs-toggle" + (blocked ? " is-on" : "")}
                  onClick={() => setBlocked((v) => !v)}
                >
                  <span className="rs-toggle-knob" />
                </button>
              </div>
            </div>
          )}

          {tab === "permissions" && (
            <div
              className="rs-fields"
              role="tabpanel"
              id={`${tabsId}-panel-permissions`}
              aria-labelledby={`${tabsId}-tab-permissions`}
            >
              <div className="rs-field rs-field--full" role="group" aria-labelledby={`${tabsId}-roles-label`}>
                <span className="rs-field-label" id={`${tabsId}-roles-label`}>
                  Roles
                </span>
                <div className="rs-perm-grid">
                  {rolesData.map((r) => (
                    <button
                      key={r.key}
                      role="checkbox"
                      aria-checked={roles.includes(r.key)}
                      className={"rs-role-radio" + (roles.includes(r.key) ? " is-on" : "")}
                      onClick={() => toggleRole(r.key)}
                      type="button"
                    >
                      <span className="rs-radio-dot" />
                      <span className="rs-role-radio-text">
                        <strong>
                          <span
                            className="rs-rolebar-dot"
                            style={{ ["--chip" as string]: r.color }}
                          />
                          {r.name}
                        </strong>
                        <span>{r.description}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            </div>
          )}

          {tab === "api" && (
            <div
              className="rs-api"
              role="tabpanel"
              id={`${tabsId}-panel-api`}
              aria-labelledby={`${tabsId}-tab-api`}
            >
              <pre className="rs-code">
                <code>
                  {JSON.stringify(
                    { id: isNew ? "—" : id, email, roles, confirmed, blocked },
                    null,
                    2,
                  )}
                </code>
              </pre>
            </div>
          )}
        </div>

        <aside className="rs-editor-rail">
          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.user size={15} /> Profile
            </div>
            <div className="rs-user-cell" style={{ marginBottom: 12 }}>
              <Avatar
                name={email || "user"}
                initials={initials(email || "?")}
                color={AVATAR_NEUTRAL}
                size={40}
              />
              <span className="rs-user-id">
                <strong>{email || "New user"}</strong>
              </span>
            </div>
            <div className="rs-rail-divider" />
            <div className="rs-rail-field">
              <label>Role</label>
              {primaryRole ? (
                <span
                  className="rs-role-pill"
                  style={{ ["--chip" as string]: primaryRole.color, alignSelf: "flex-start" }}
                >
                  {primaryRole.name}
                </span>
              ) : (
                <span className="rs-cell-muted">No role</span>
              )}
            </div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.lock size={15} /> Security
            </div>
            <div className="rs-rail-stat">
              <span>Provider</span>
              <strong>Email</strong>
            </div>
            <div className="rs-rail-stat">
              <span>2FA</span>
              <strong>Disabled</strong>
            </div>
            <div className="rs-rail-stat">
              <span>Confirmed</span>
              <strong>{confirmed ? "Yes" : "No"}</strong>
            </div>
          </div>

          {!isNew && existing && (
            <div className="rs-rail-card">
              <div className="rs-rail-card-head">
                <Icons.clock size={15} /> Activity
              </div>
              <div className="rs-rail-stat">
                <span>Joined</span>
                <strong>{relTime(existing.created_at)}</strong>
              </div>
              <div className="rs-rail-stat">
                <span>User ID</span>
                <strong className="rs-mono">{existing.id.slice(0, 8)}</strong>
              </div>
            </div>
          )}
        </aside>
      </div>
      {confirmDel && (
        <ConfirmDialog
          title="Delete this user?"
          body="This cannot be undone."
          confirmLabel="Delete user"
          busy={busy}
          onConfirm={remove}
          onCancel={() => setConfirmDel(false)}
        />
      )}
    </div>
  );
}
