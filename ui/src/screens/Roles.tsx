import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { useResource } from "../hooks/useResource";
import { listUsers } from "../api/endpoints";
import { ROLES } from "../roles";
import type { User } from "../api/types";
import { initials, AVATAR_NEUTRAL } from "../util";

export function Roles() {
  const navigate = useNavigate();
  const users = useResource(() => listUsers(), []);
  const [query, setQuery] = useState("");

  const all = users.data ?? [];
  const totalAssigned = all.filter((u) => u.roles.length > 0).length;

  const rows = ROLES.filter(
    (r) =>
      !query ||
      r.name.toLowerCase().includes(query.toLowerCase()) ||
      r.desc.toLowerCase().includes(query.toLowerCase()),
  );

  const membersOf = (key: string): User[] => all.filter((u) => u.roles.includes(key));

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Roles</h1>
          <p className="rs-cm-sub">
            {ROLES.length} roles &middot;{" "}
            {users.loading ? "…" : users.error ? "—" : totalAssigned} user
            {totalAssigned === 1 ? "" : "s"} assigned
          </p>
        </div>
        <button className="rs-btn rs-btn--primary" data-placeholder title="Coming soon" disabled>
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
        <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon" disabled>
          <Icons.eye size={15} /> Permissions reference
        </button>
      </div>

      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th>Role</th>
              <th>Description</th>
              <th>Users</th>
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
                      <span className="rs-rolebar-dot" style={{ ["--chip" as string]: r.color }} />
                      {r.name}
                      <span className="rs-role-system">System</span>
                    </span>
                  </td>
                  <td className="rs-role-desc">{r.desc}</td>
                  <td>
                    {users.loading ? (
                      <span className="rs-cell-muted">…</span>
                    ) : users.error ? (
                      <span className="rs-cell-muted">—</span>
                    ) : members.length === 0 ? (
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
                </tr>
              );
            })}
          </tbody>
        </table>
        {rows.length === 0 && <div className="rs-empty">No roles match your search.</div>}
      </div>
    </div>
  );
}
