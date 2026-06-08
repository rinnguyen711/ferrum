import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { EditorBar, LoadingState, EmptyState } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listUsers } from "../api/endpoints";
import { ROLES, roleOf, CAPS, capsFor } from "../roles";
import { shortId, initials, AVATAR_NEUTRAL } from "../util";

export function RoleDetail() {
  const { key } = useParams<{ key: string }>();
  const navigate = useNavigate();

  const users = useResource(() => listUsers(), []);

  const isValid = ROLES.some((r) => r.key === key);

  if (!isValid) {
    return (
      <div className="rs-editor">
        <EditorBar onBack={() => navigate("/roles")} title="Role not found" />
        <div className="rs-editor-body">
          <div className="rs-editor-main">
            <EmptyState>
              Role "{key}" does not exist.{" "}
              <button className="rs-btn rs-btn--ghost" onClick={() => navigate("/roles")}>
                Back to Roles
              </button>
            </EmptyState>
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
      <EditorBar
        onBack={() => navigate("/roles")}
        title={
          <span className="rs-role-name">
            <span className="rs-rolebar-dot" style={{ ["--chip" as string]: role.color }} />
            {role.name}
            <span className="rs-role-system">System</span>
          </span>
        }
      />

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
              {users.loading && <LoadingState />}
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
                                initials={initials(u.email)}
                                color={AVATAR_NEUTRAL}
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
