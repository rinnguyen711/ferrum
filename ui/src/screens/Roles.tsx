import { useNavigate } from "react-router-dom";
import { useResource } from "../hooks/useResource";
import { listUsers } from "../api/endpoints";
import { ROLES } from "../roles";

export function Roles() {
  const navigate = useNavigate();
  const users = useResource(() => listUsers(), []);

  const all = users.data ?? [];
  const totalAssigned = all.filter((u) => u.roles.length > 0).length;

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Roles</h1>
          <p className="rs-cm-sub">
            {ROLES.length} roles &middot;{" "}
            {users.loading ? "…" : users.error ? "—" : totalAssigned} assigned
          </p>
        </div>
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
            {ROLES.map((r) => {
              const count = all.filter((u) => u.roles.includes(r.key)).length;
              return (
                <tr key={r.key} onClick={() => navigate("/roles/" + r.key)}>
                  <td>
                    <span className="rs-role-pill" style={{ ["--chip" as string]: r.color }}>
                      <span className="rs-rolebar-dot" style={{ ["--chip" as string]: r.color }} />
                      {r.name}
                    </span>
                  </td>
                  <td>{r.desc}</td>
                  <td>
                    {users.loading ? (
                      <span className="rs-cell-muted">…</span>
                    ) : users.error ? (
                      <span className="rs-cell-muted">—</span>
                    ) : (
                      count
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
