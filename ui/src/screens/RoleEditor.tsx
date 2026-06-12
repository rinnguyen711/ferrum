import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { EditorBar } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listContentTypes, listUsers, getRole, createRole, updateRole } from "../api/endpoints";
import type { RolePermission } from "../api/types";
import {
  PLUGIN_TYPES,
  CONTENT_VERBS,
  ROLE_COLORS,
  DEFAULT_ROLE_COLOR,
  type PermType,
} from "../roles";
import { initials, AVATAR_NEUTRAL } from "../util";

const permKey = (type: string, verb: string) => `${type}::${verb}`;

/** Scope string shown under each permission group (display only — the API value
 *  is the raw content_type). Plugin pseudo-types already carry their namespace. */
const scopeOf = (key: string) => (key.startsWith("plugin::") ? key : `api::${key}`);

type Tab = "permissions" | "members" | "api";

export function RoleEditor() {
  const { key } = useParams<{ key: string }>();
  const navigate = useNavigate();
  const isNew = key === undefined;

  const types = useResource(() => listContentTypes(), []);
  const users = useResource(() => listUsers(), []);
  const role = useResource(() => (isNew ? Promise.resolve(null) : getRole(key!)), [key]);

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [color, setColor] = useState(DEFAULT_ROLE_COLOR);
  const [keyField, setKeyField] = useState("");
  const [granted, setGranted] = useState<Set<string>>(new Set());
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [tab, setTab] = useState<Tab>("permissions");
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isSystem = role.data?.is_system ?? false;

  useEffect(() => {
    if (role.data) {
      setName(role.data.name);
      setDescription(role.data.description);
      setColor(role.data.color);
      setKeyField(role.data.key);
      setGranted(new Set(role.data.permissions.map((p) => permKey(p.content_type, p.action))));
      setDirty(false);
    }
  }, [role.data]);

  const permTypes: PermType[] = useMemo(() => {
    const contentTypes: PermType[] = (types.data ?? []).map((t) => ({
      key: t.name,
      label: t.display_name ?? t.name,
      icon: "doc",
      verbs: [...CONTENT_VERBS],
    }));
    return [...contentTypes, ...PLUGIN_TYPES];
  }, [types.data]);

  // First group open by default once the matrix is known.
  useEffect(() => {
    if (permTypes.length && open.size === 0) setOpen(new Set([permTypes[0].key]));
  }, [permTypes]); // eslint-disable-line react-hooks/exhaustive-deps

  const members = useMemo(
    () => (users.data ?? []).filter((u) => key && u.roles.includes(key)),
    [users.data, key],
  );

  const totalActions = permTypes.reduce((n, t) => n + t.verbs.length, 0);
  const totalEnabled = granted.size;

  const mutate = (fn: () => void) => {
    if (isSystem) return;
    fn();
    setDirty(true);
  };

  const toggleOpen = (k: string) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });

  const toggle = (type: string, verb: string) =>
    mutate(() => {
      setGranted((prev) => {
        const next = new Set(prev);
        const k = permKey(type, verb);
        if (next.has(k)) next.delete(k);
        else next.add(k);
        return next;
      });
    });

  const toggleAll = (t: PermType) =>
    mutate(() => {
      setGranted((prev) => {
        const next = new Set(prev);
        const full = t.verbs.every((v) => next.has(permKey(t.key, v)));
        t.verbs.forEach((v) => (full ? next.delete(permKey(t.key, v)) : next.add(permKey(t.key, v))));
        return next;
      });
    });

  const toPermissions = (): RolePermission[] => {
    const out: RolePermission[] = [];
    for (const k of granted) {
      const [content_type, action] = k.split("::");
      out.push({ content_type, action });
    }
    return out;
  };

  const slug = (s: string) =>
    s.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");

  const discard = () => {
    if (role.data) {
      setName(role.data.name);
      setDescription(role.data.description);
      setColor(role.data.color);
      setGranted(new Set(role.data.permissions.map((p) => permKey(p.content_type, p.action))));
    } else {
      setName("");
      setDescription("");
      setColor(DEFAULT_ROLE_COLOR);
      setKeyField("");
      setGranted(new Set());
    }
    setDirty(false);
    setError(null);
  };

  const onSave = async () => {
    setError(null);
    const finalKey = isNew ? slug(keyField || name) : key!;
    if (!name.trim()) {
      setError("Name is required.");
      return;
    }
    if (isNew && !finalKey) {
      setError("Key is required.");
      return;
    }
    setSaving(true);
    try {
      if (isNew) {
        await createRole({
          key: finalKey,
          name: name.trim(),
          description,
          color,
          permissions: toPermissions(),
        });
      } else {
        await updateRole(key!, {
          name: name.trim(),
          description,
          color,
          permissions: toPermissions(),
        });
      }
      setDirty(false);
      navigate("/roles");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save role.");
    } finally {
      setSaving(false);
    }
  };

  const tabs: [Tab, string][] = [
    ["permissions", "Permissions"],
    ["members", `Members · ${members.length}`],
    ["api", "API & preview"],
  ];

  return (
    <div className="rs-editor">
      <EditorBar
        onBack={() => navigate("/roles")}
        title={isNew ? "Create a role" : name || "Untitled role"}
        status={
          <div className="rs-editor-meta">
            <span className="rs-type-pill">Role</span>
            {!isNew && (
              <span className="rs-cell-muted">
                · {members.length} user{members.length === 1 ? "" : "s"} · {totalEnabled}/
                {totalActions} permissions
              </span>
            )}
            {isSystem && <span className="rs-cell-muted">· System role</span>}
            {dirty && (
              <span className="rs-unsaved">
                <span className="rs-dot" /> Unsaved changes
              </span>
            )}
          </div>
        }
        actions={
          <>
            <button
              className="rs-btn rs-btn--ghost"
              onClick={discard}
              disabled={!dirty || saving}
            >
              Discard
            </button>
            <button
              className="rs-btn rs-btn--primary"
              onClick={onSave}
              disabled={saving || isSystem || (!dirty && !isNew)}
            >
              {saving ? (
                <>
                  <Icons.spinner size={14} className="rs-spin" /> Saving…
                </>
              ) : (
                <>
                  <Icons.bolt size={15} /> {isNew ? "Create role" : "Save role"}
                </>
              )}
            </button>
          </>
        }
      />

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-editor-tabs">
            {tabs.map(([k, label]) => (
              <button
                key={k}
                className={"rs-etab" + (tab === k ? " is-active" : "")}
                onClick={() => setTab(k)}
              >
                {label}
              </button>
            ))}
          </div>

          {error && <div className="rs-banner rs-banner--error">{error}</div>}
          {isSystem && (
            <div className="rs-banner">This is a system role and cannot be edited.</div>
          )}

          {tab === "permissions" && (
            <div className="rs-fields">
              <div className="rs-field">
                <span className="rs-field-label">
                  Name <span className="rs-req">*</span>
                  <span className="rs-field-hint">Shown when assigning users</span>
                </span>
                <input
                  className="rs-input rs-input--lg"
                  value={name}
                  placeholder="Role name"
                  disabled={isSystem}
                  onChange={(e) => mutate(() => setName(e.target.value))}
                />
              </div>

              {isNew && (
                <div className="rs-field">
                  <span className="rs-field-label">Key</span>
                  <input
                    className="rs-input rs-mono"
                    value={keyField}
                    placeholder={slug(name) || "role-key"}
                    onChange={(e) => mutate(() => setKeyField(e.target.value))}
                  />
                </div>
              )}

              <div className="rs-field">
                <span className="rs-field-label">
                  Description <span className="rs-field-hint">What this role is for</span>
                </span>
                <textarea
                  className="rs-input rs-textarea"
                  rows={2}
                  value={description}
                  placeholder="Describe the access this role grants."
                  disabled={isSystem}
                  onChange={(e) => mutate(() => setDescription(e.target.value))}
                />
              </div>

              <div className="rs-field">
                <span className="rs-field-label">Color</span>
                <div className="rs-color-swatches">
                  {ROLE_COLORS.map((c) => (
                    <button
                      key={c}
                      type="button"
                      className={"rs-swatch" + (color === c ? " is-active" : "")}
                      style={{ background: c }}
                      disabled={isSystem}
                      onClick={() => mutate(() => setColor(c))}
                      aria-label={c}
                    />
                  ))}
                </div>
              </div>

              <div className="rs-field">
                <span className="rs-field-label">
                  Permissions
                  <span className="rs-field-hint">
                    {totalEnabled} of {totalActions} actions enabled across types &amp; plugins
                  </span>
                </span>
                <div className="rs-perm-list rs-perm-cards">
                  {permTypes.map((t) => {
                    const full = t.verbs.every((v) => granted.has(permKey(t.key, v)));
                    const on = t.verbs.filter((v) => granted.has(permKey(t.key, v))).length;
                    const isOpen = open.has(t.key);
                    const I = (Icons as Record<string, typeof Icons.doc>)[t.icon] ?? Icons.doc;
                    return (
                      <div
                        className={"rs-perm-type" + (isOpen ? " is-open" : "")}
                        key={t.key}
                      >
                        <button
                          type="button"
                          className="rs-perm-type-head"
                          onClick={() => toggleOpen(t.key)}
                        >
                          <span className="rs-perm-chev">
                            <Icons.chevRight size={16} />
                          </span>
                          <span className="rs-perm-type-icon">
                            <I size={16} />
                          </span>
                          <span className="rs-perm-type-meta">
                            <strong>{t.label}</strong>
                            <code className="rs-mono">{scopeOf(t.key)}</code>
                          </span>
                          <span className={"rs-perm-tally" + (on > 0 ? " is-full" : "")}>
                            {on}/{t.verbs.length}
                          </span>
                          <span
                            role="button"
                            className="rs-btn rs-btn--ghost rs-btn--sm"
                            onClick={(e) => {
                              e.stopPropagation();
                              toggleAll(t);
                            }}
                          >
                            {full ? "Clear" : "All"}
                          </span>
                        </button>
                        {isOpen && (
                          <div className="rs-perm-type-grid">
                            {t.verbs.map((v) => {
                              const checked = granted.has(permKey(t.key, v));
                              return (
                                <label className="rs-perm-action" key={v}>
                                  <input
                                    type="checkbox"
                                    checked={checked}
                                    disabled={isSystem}
                                    onChange={() => toggle(t.key, v)}
                                  />
                                  <span className="rs-mono">{v}</span>
                                </label>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          )}

          {tab === "members" && (
            <div className="rs-table-wrap">
              {members.length === 0 ? (
                <div className="rs-empty">No users have this role yet.</div>
              ) : (
                <table className="rs-table">
                  <tbody>
                    {members.map((u) => (
                      <tr key={u.id} onClick={() => navigate(`/users/${u.id}`)}>
                        <td>
                          <span className="rs-user-cell">
                            <Avatar
                              name={u.email}
                              initials={initials(u.email)}
                              color={AVATAR_NEUTRAL}
                              size={32}
                            />
                            <span className="rs-user-id">
                              <strong>{u.email}</strong>
                            </span>
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}

          {tab === "api" && (
            <div className="rs-api">
              <div className="rs-api-note">
                <Icons.bolt size={14} /> Permissions are resolved from an in-memory cache in the
                Rust API and reloaded on save — no per-request database lookups.
              </div>
              <pre className="rs-code">
                <code>
                  {JSON.stringify(
                    {
                      key: isNew ? slug(keyField || name) || "—" : key,
                      name,
                      description,
                      permissions: toPermissions(),
                    },
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
              <Icons.user size={15} /> Role
            </div>
            <div className="rs-rail-field">
              <label>Name</label>
              <span
                className="rs-role-pill"
                style={{ ["--chip" as string]: color, alignSelf: "flex-start" }}
              >
                {name || "Untitled"}
              </span>
            </div>
            <div className="rs-rail-divider" />
            <div className="rs-rail-stat">
              <span>Permissions</span>
              <strong>
                {totalEnabled} / {totalActions}
              </strong>
            </div>
            <div className="rs-rail-stat">
              <span>Users</span>
              <strong>{members.length}</strong>
            </div>
            <div className="rs-rail-stat">
              <span>Type</span>
              <strong>{isSystem ? "System" : "Custom"}</strong>
            </div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.user size={15} /> Members <span className="rs-rel-count">{members.length}</span>
            </div>
            {members.length === 0 ? (
              <p className="rs-rel-empty">No users assigned to this role yet.</p>
            ) : (
              <div className="rs-rel-list">
                {members.slice(0, 6).map((u) => (
                  <button
                    key={u.id}
                    className="rs-rel-item"
                    onClick={() => navigate(`/users/${u.id}`)}
                  >
                    <Avatar
                      name={u.email}
                      initials={initials(u.email)}
                      color={AVATAR_NEUTRAL}
                      size={24}
                    />
                    <span className="rs-rel-item-title">{u.email}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}
