import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Notice, EditorBar } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listUsers, listRoles, createUser, updateUser, deleteUser } from "../api/endpoints";
import { ApiError } from "../api/client";

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
  const [tab, setTab] = useState<"account" | "permissions">("account");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [hydrated, setHydrated] = useState(false);

  // Hydrate form once the user loads (edit mode).
  if (!isNew && existing && !hydrated) {
    setEmail(existing.email);
    setRoles(existing.roles.length ? existing.roles : ["viewer"]);
    setHydrated(true);
  }

  const toggleRole = (key: string) => {
    setRoles((rs) => (rs.includes(key) ? rs.filter((r) => r !== key) : [...rs, key]));
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      if (isNew) {
        await createUser({ email, password, roles });
      } else {
        const patch: { email?: string; password?: string; roles?: string[] } = { email, roles };
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
    if (!window.confirm("Delete this user? This cannot be undone.")) return;
    setBusy(true);
    setError(null);
    try {
      await deleteUser(id);
      navigate("/users");
    } catch (e) {
      if (e instanceof ApiError) setError(e.message);
      else setError("Something went wrong.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rs-editor">
      <EditorBar
        onBack={() => navigate("/users")}
        title={isNew ? "Add a user" : email || "User"}
        actions={
          <>
            {!isNew && (
              <button className="rs-btn rs-btn--ghost rs-danger" disabled={busy} onClick={remove}>
                <Icons.trash size={15} /> Delete
              </button>
            )}
            <button className="rs-btn rs-btn--primary" disabled={busy || !email || (isNew && !password)} onClick={save}>
              <Icons.check size={15} /> {isNew ? "Create user" : "Save user"}
            </button>
          </>
        }
      />

      {error && <div style={{ margin: "0 24px" }}><Notice>{error}</Notice></div>}

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-editor-tabs">
            {([["account", "Account"], ["permissions", "Role & permissions"]] as const).map(([k, l]) => (
              <button key={k} className={"rs-etab" + (tab === k ? " is-active" : "")} onClick={() => setTab(k)}>
                {l}
              </button>
            ))}
          </div>

          {tab === "account" && (
            <div className="rs-fields">
              <label className="rs-field">
                <span className="rs-field-label">Email</span>
                <input className="rs-input rs-mono" type="email" value={email} placeholder="name@company.com"
                  onChange={(e) => setEmail(e.target.value)} />
              </label>
              <label className="rs-field">
                <span className="rs-field-label">{isNew ? "Password" : "Reset password"}</span>
                <input className="rs-input" type="password" value={password}
                  placeholder={isNew ? "At least 8 characters" : "Leave blank to keep current"}
                  onChange={(e) => setPassword(e.target.value)} />
              </label>
            </div>
          )}

          {tab === "permissions" && (
            <div className="rs-fields">
              <div className="rs-field">
                <span className="rs-field-label">Roles</span>
                <div className="rs-perm-grid">
                  {rolesData.map((r) => (
                    <button key={r.key} className={"rs-role-radio" + (roles.includes(r.key) ? " is-on" : "")}
                      onClick={() => toggleRole(r.key)} type="button">
                      <span className="rs-radio-dot" />
                      <span className="rs-role-radio-text">
                        <strong><span className="rs-rolebar-dot" style={{ ["--chip" as string]: r.color }} />{r.name}</strong>
                        <span>{r.description}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>
              <div className="rs-field" data-placeholder title="Coming soon">
                <span className="rs-field-label">Two-factor authentication</span>
                <span className="rs-cell-muted">Coming soon</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
