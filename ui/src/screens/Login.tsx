import { useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { setKey } from "../auth";
import { checkAuth } from "../api/endpoints";

export function Login() {
  const navigate = useNavigate();
  const location = useLocation();
  const from = (location.state as { from?: string } | null)?.from ?? "/";

  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!value || busy) return;
    setBusy(true);
    setError(null);
    try {
      const ok = await checkAuth(value);
      if (!ok) {
        setError("Invalid admin key.");
        return;
      }
      setKey(value);
      navigate(from, { replace: true });
    } catch {
      setError("Can't reach the API.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rs-login">
      <form className="rs-login-card" onSubmit={submit}>
        <h1>Rustapi Studio</h1>
        <p className="rs-cell-muted">Enter your admin API key to continue.</p>
        <input
          className="rs-input rs-mono"
          type="password"
          placeholder="x-api-key"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          autoFocus
        />
        {error && <div className="rs-login-error">{error}</div>}
        <button className="rs-btn rs-btn--primary" type="submit" disabled={busy || !value}>
          {busy ? "Checking…" : "Sign in"}
        </button>
      </form>
    </div>
  );
}
