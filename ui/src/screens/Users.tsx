import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { LoadingState, EmptyState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listUsers, listRoles } from "../api/endpoints";
import { shortId, initials, AVATAR_NEUTRAL } from "../util";

export function Users() {
  const navigate = useNavigate();
  const users = useResource(() => listUsers(), []);
  const roles = useResource(() => listRoles(), []);
  const rolesData = roles.data ?? [];
  const roleOf = (key: string) => rolesData.find((r) => r.key === key);
  const [query, setQuery] = useState("");
  const [roleFilter, setRoleFilter] = useState("all");

  const all = users.data ?? [];
  const rows = all.filter((u) => {
    if (roleFilter !== "all" && !u.roles.includes(roleFilter)) return false;
    if (query && !u.email.toLowerCase().includes(query.toLowerCase())) return false;
    return true;
  });

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Users</h1>
          <p className="rs-cm-sub">{all.length} member{all.length === 1 ? "" : "s"}</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => navigate("/users/new")}>
          <Icons.plus size={16} /> Add user
        </button>
      </div>

      <div className="rs-rolebar">
        {rolesData.map((r) => (
          <button
            key={r.key}
            className={"rs-rolebar-item" + (roleFilter === r.key ? " is-active" : "")}
            style={{ ["--chip" as string]: r.color }}
            onClick={() => setRoleFilter(roleFilter === r.key ? "all" : r.key)}
            title={r.description}
          >
            <span className="rs-rolebar-dot" />
            <strong>{r.name}</strong>
            <span>{all.filter((u) => u.roles.includes(r.key)).length}</span>
          </button>
        ))}
      </div>

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input placeholder="Search email" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon" disabled>
          <Icons.external size={15} /> Export
        </button>
      </div>

      {users.loading && <LoadingState />}
      {users.error && <EmptyState>Couldn't load users.</EmptyState>}

      {!users.loading && !users.error && (
        <div className="rs-table-wrap">
          <table className="rs-table">
            <thead>
              <tr>
                <th className="rs-col-id">ID</th>
                <th>User</th>
                <th>Roles</th>
                <th className="rs-col-2fa" data-placeholder title="Coming soon">2FA</th>
                <th className="rs-col-act"></th>
              </tr>
            </thead>
            <tbody>
              {rows.map((u) => (
                <tr key={u.id} onClick={() => navigate(`/users/${u.id}`)}>
                  <td className="rs-col-id rs-mono">{shortId(u.id)}</td>
                  <td>
                    <span className="rs-user-cell">
                      <Avatar name={u.email} initials={initials(u.email)} color={AVATAR_NEUTRAL} size={34} />
                      <span className="rs-user-id">
                        <strong>{u.email}</strong>
                      </span>
                    </span>
                  </td>
                  <td>
                    {u.roles.length === 0 && <span className="rs-cell-muted">—</span>}
                    {u.roles.map((rk) => {
                      const r = roleOf(rk);
                      return (
                        <span
                          key={rk}
                          className="rs-role-pill"
                          style={{ ["--chip" as string]: r?.color ?? "#52525B" }}
                        >
                          {r?.name ?? rk}
                        </span>
                      );
                    })}
                  </td>
                  <td className="rs-col-2fa" data-placeholder>
                    <span className="rs-2fa is-off"><span className="rs-2fa-dash" /> Off</span>
                  </td>
                  <td className="rs-col-act" onClick={(e) => e.stopPropagation()}>
                    <button className="rs-row-btn" onClick={() => navigate(`/users/${u.id}`)}>
                      <Icons.edit size={16} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {rows.length === 0 && <div className="rs-empty">No users match.</div>}
        </div>
      )}
    </div>
  );
}
