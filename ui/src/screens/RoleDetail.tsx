import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { useResource } from "../hooks/useResource";
import { listUsers } from "../api/endpoints";
import { ROLES, roleOf, CAPS, capsFor } from "../roles";
import { shortId } from "../util";

export function RoleDetail() {
  const { key } = useParams<{ key: string }>();
  const navigate = useNavigate();

  const users = useResource(() => listUsers(), []);

  const isValid = ROLES.some((r) => r.key === key);

  if (!isValid) {
    return (
      <div className="rs-editor">
        <div className="rs-editor-bar">
          <button className="rs-back" onClick={() => navigate("/roles")}>
            <Icons.arrowLeft size={18} />
          </button>
          <div className="rs-editor-titlewrap">
            <h1>Role not found</h1>
          </div>
        </div>
        <div className="rs-editor-body">
          <div className="rs-editor-main">
            <div className="rs-empty">
              Role "{key}" does not exist.{" "}
              <button className="rs-btn rs-btn--ghost" onClick={() => navigate("/roles")}>
                Back to Roles
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  const role = roleOf(key!);
  const caps = capsFor(role.key);
  const members = (users.data ?? []).filter((u) => u.roles.includes(role.key));

  return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={() => navigate("/roles")}>
          <Icons.arrowLeft size={18} />
        </button>
        <div className="rs-editor-titlewrap">
          <h1>
            <span className="rs-role-name">
              <span className="rs-rolebar-dot" style={{ ["--chip" as string]: role.color }} />
              {role.name}
              <span className="rs-role-system">System</span>
            </span>
          </h1>
        </div>
      </div>

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-fields">
            <div className="rs-field">
              <span className="rs-field-label">Capabilities</span>
              <div className="rs-cap">
                {CAPS.map((c, i) => {
                  const on = caps[i];
                  return (
                    <div className="rs-cap-row" key={c}>
                      <span>{c}</span>
                      <span className={"rs-cap-mark " + (on ? "is-on" : "is-off")}>
                        {on ? <Icons.check size={13} /> : <Icons.x size={12} />}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>

            <div className="rs-field">
              <span className="rs-field-label">Members</span>
              {users.loading && <div className="rs-empty">Loading…</div>}
              {(!users.loading && members.length === 0) && (
                <div className="rs-empty rs-cell-muted">No users have this role yet.</div>
              )}
              {!users.loading && members.length > 0 && (
                <div className="rs-table-wrap">
                  <table className="rs-table">
                    <tbody>
                      {members.map((u) => (
                        <tr key={u.id} onClick={() => navigate(`/users/${u.id}`)}>
                          <td className="rs-col-id rs-mono">{shortId(u.id)}</td>
                          <td>
                            <span className="rs-user-cell">
                              <Avatar
                                name={u.email}
                                initials={u.email.slice(0, 2).toUpperCase()}
                                color="#52525B"
                                size={34}
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
                </div>
              )}
            </div>

            <div className="rs-field">
              <span className="rs-cell-muted">
                Roles are defined in the API and cannot be edited here yet.
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
