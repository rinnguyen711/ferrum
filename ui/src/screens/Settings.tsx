import { useNavigate } from "react-router-dom";
import { clearToken } from "../auth";

export function Settings() {
  const navigate = useNavigate();
  const signOut = () => {
    clearToken();
    navigate("/login", { replace: true });
  };
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Settings</h1>
        </div>
      </div>
      <div className="rs-setting-row">
        <div className="rs-setting-meta">
          <strong>Session</strong>
          <span className="rs-cell-muted">
            Your admin key is stored in this browser. Sign out to clear it.
          </span>
        </div>
        <button className="rs-btn rs-btn--ghost rs-danger" onClick={signOut}>
          Sign out
        </button>
      </div>
    </div>
  );
}
