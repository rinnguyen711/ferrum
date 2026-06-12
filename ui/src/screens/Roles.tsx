import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { LoadingState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listRoles, listUsers, deleteRole } from "../api/endpoints";
import { initials, AVATAR_NEUTRAL } from "../util";

export function Roles() {
  const navigate = useNavigate();
  const roles = useResource(() => listRoles(), []);
  const users = useResource(() => listUsers(), []);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);

  const allRoles = roles.data ?? [];
  const allUsers = users.data ?? [];
  const totalAssigned = allUsers.filter((u) => u.roles.length > 0).length;

  const rows = allRoles.filter(
    (r) =>
      !query ||
      r.name.toLowerCase().includes(query.toLowerCase()) ||
      r.description.toLowerCase().includes(query.toLowerCase()),
  );

  const membersOf = (key: string) => allUsers.filter((u) => u.roles.includes(key));

  const onDelete = async (key: string, name: string) => {
    if (!window.confirm(`Delete role "${name}"? This cannot be undone.`)) return;
    setBusy(true);
    try {
      await deleteRole(key);
      roles.refetch();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Roles</h1>
          <p className="rs-cm-sub">
            {allRoles.length} roles ·{" "}
            {users.loading ? "…" : totalAssigned} user{totalAssigned === 1 ? "" : "s"} assigned
          </p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => navigate("/roles/new")}>
          <Icons.plus size={16} /> Create new role
        </button>
      </div>

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input
            placeholder="Search roles"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="rs-spacer" />
      </div>

      {roles.loading ? (
        <LoadingState />
      ) : (
        <div className="rs-table-wrap">
          <table className="rs-table">
            <thead>
              <tr>
                <th>Role</th>
                <th>Description</th>
                <th>Users</th>
                <th className="rs-col-act"></th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const members = membersOf(r.key);
                const shown = members.slice(0, 4);
                return (
                  <tr key={r.key} onClick={() => navigate("/roles/" + r.key)}>
                    <td>
                      <span className="rs-role-name">
                        <span
                          className="rs-rolebar-dot"
                          style={{ ["--chip" as string]: r.color }}
                        />
                        {r.name}
                        {r.is_system && <span className="rs-role-system">System</span>}
                      </span>
                    </td>
                    <td className="rs-role-desc">{r.description}</td>
                    <td>
                      {members.length === 0 ? (
                        <span className="rs-cell-muted">No users</span>
                      ) : (
                        <span className="rs-avatar-stack">
                          {shown.map((u) => (
                            <Avatar
                              key={u.id}
                              name={u.email}
                              initials={initials(u.email)}
                              color={AVATAR_NEUTRAL}
                              size={26}
                            />
                          ))}
                          <span className="rs-avatar-more">
                            {members.length} user{members.length === 1 ? "" : "s"}
                          </span>
                        </span>
                      )}
                    </td>
                    <td className="rs-col-act" onClick={(e) => e.stopPropagation()}>
                      <button
                        className="rs-row-btn"
                        title="Edit"
                        onClick={() => navigate("/roles/" + r.key)}
                      >
                        <Icons.edit size={16} />
                      </button>
                      <button
                        className={"rs-row-btn" + (r.is_system ? "" : " rs-danger")}
                        disabled={r.is_system || busy}
                        title={r.is_system ? "System roles can't be deleted" : "Delete"}
                        onClick={() => onDelete(r.key, r.name)}
                      >
                        <Icons.trash size={16} />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {rows.length === 0 && <div className="rs-empty">No roles match your search.</div>}
        </div>
      )}
    </div>
  );
}
