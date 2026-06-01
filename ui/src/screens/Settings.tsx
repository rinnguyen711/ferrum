import { useNavigate } from "react-router-dom";
import { clearKey } from "../auth";

export function Settings() {
  const navigate = useNavigate();
  const logout = () => {
    clearKey();
    navigate("/login", { replace: true });
  };
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Settings</h1>
          <p className="rs-cm-sub">Session</p>
        </div>
        <button className="rs-btn rs-btn--ghost rs-danger" onClick={logout}>
          Sign out
        </button>
      </div>
      <div className="rs-empty">
        Admin key is stored in this browser. Sign out to clear it.
      </div>
    </div>
  );
}
