import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { EditorBar } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listContentTypes, getRole, createRole, updateRole } from "../api/endpoints";
import type { RolePermission } from "../api/types";
import {
  PLUGIN_TYPES,
  CONTENT_VERBS,
  ROLE_COLORS,
  DEFAULT_ROLE_COLOR,
  type PermType,
} from "../roles";

const permKey = (type: string, verb: string) => `${type}::${verb}`;

export function RoleEditor() {
  const { key } = useParams<{ key: string }>();
  const navigate = useNavigate();
  const isNew = key === undefined;

  const types = useResource(() => listContentTypes(), []);
  const role = useResource(() => (isNew ? Promise.resolve(null) : getRole(key!)), [key]);

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [color, setColor] = useState(DEFAULT_ROLE_COLOR);
  const [keyField, setKeyField] = useState("");
  const [granted, setGranted] = useState<Set<string>>(new Set());
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

  const totalActions = permTypes.reduce((n, t) => n + t.verbs.length, 0);
  const totalEnabled = granted.size;

  const mutate = (fn: () => void) => {
    if (isSystem) return;
    fn();
    setDirty(true);
  };

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

  return (
    <div className="rs-editor">
      <EditorBar
        onBack={() => navigate("/roles")}
        title={isNew ? "Create a role" : name || "Untitled role"}
        actions={
          <>
            {dirty && (
              <span className="rs-unsaved">
                <span className="rs-dot" /> Unsaved changes
              </span>
            )}
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
                  <Icons.save size={14} /> {isNew ? "Create role" : "Save role"}
                </>
              )}
            </button>
          </>
        }
      />

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          {error && <div className="rs-banner rs-banner--error">{error}</div>}
          {isSystem && <div className="rs-banner">This is a system role and cannot be edited.</div>}

          <div className="rs-fields">
            <div className="rs-field">
              <span className="rs-field-label">Name</span>
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
              <span className="rs-field-label">Description</span>
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
                <span className="rs-cell-muted">
                  {" "}
                  · {totalEnabled} of {totalActions} actions
                </span>
              </span>
              <div className="rs-perm-list">
                {permTypes.map((t) => {
                  const full = t.verbs.every((v) => granted.has(permKey(t.key, v)));
                  const on = t.verbs.filter((v) => granted.has(permKey(t.key, v))).length;
                  const I = (Icons as Record<string, typeof Icons.doc>)[t.icon] ?? Icons.doc;
                  return (
                    <div className="rs-perm-type is-open" key={t.key}>
                      <div className="rs-perm-type-head">
                        <span className="rs-perm-type-icon">
                          <I size={16} />
                        </span>
                        <span className="rs-perm-type-meta">
                          <strong>{t.label}</strong>
                          <code className="rs-mono">{t.key}</code>
                        </span>
                        <span className={"rs-perm-tally" + (full ? " is-full" : "")}>
                          {on}/{t.verbs.length}
                        </span>
                        <button
                          type="button"
                          className="rs-btn rs-btn--ghost rs-btn--sm"
                          disabled={isSystem}
                          onClick={() => toggleAll(t)}
                        >
                          {full ? "Clear" : "All"}
                        </button>
                      </div>
                      <div className="rs-perm-type-body">
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
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
