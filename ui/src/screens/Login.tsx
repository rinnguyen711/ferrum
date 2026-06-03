import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { setToken } from "../auth";
import { fetchSetupStatus, login, setup } from "../api/endpoints";
import { ApiError } from "../api/client";

type Mode = "loading" | "login" | "setup" | "unreachable";

export function Login() {
  const navigate = useNavigate();
  const location = useLocation();
  const from = (location.state as { from?: string } | null)?.from ?? "/";

  const [mode, setMode] = useState<Mode>("loading");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const probe = () => {
    setMode("loading");
    setError(null);
    fetchSetupStatus()
      .then((s) => setMode(s.setup_required ? "setup" : "login"))
      .catch(() => setMode("unreachable"));
  };

  useEffect(probe, []);

  const finishLogin = async (e: string, p: string) => {
    const res = await login(e, p);
    setToken(res.token);
    navigate(from, { replace: true });
  };

  const submit = async (ev: React.FormEvent) => {
    ev.preventDefault();
    if (busy) return;
    setError(null);

    if (mode === "setup" && password !== confirm) {
      setError("Passwords do not match.");
      return;
    }

    setBusy(true);
    try {
      if (mode === "setup") {
        try {
          await setup(email, password);
        } catch (e) {
          if (e instanceof ApiError && e.status === 409) {
            setMode("login");
            setError("Admin already exists — please sign in.");
            return;
          }
          throw e;
        }
        await finishLogin(email, password);
      } else {
        await finishLogin(email, password);
      }
    } catch (e) {
      if (e instanceof ApiError) {
        if (e.status === 401) setError("Invalid email or password.");
        else if (e.status === 0) setError("Can't reach the API.");
        else if (e.fieldErrors.length) setError(e.fieldErrors[0].message ?? e.message);
        else setError(e.message);
      } else {
        setError("Something went wrong.");
      }
    } finally {
      setBusy(false);
    }
  };

  if (mode === "loading") {
    return (
      <div className="rs-login">
        <div className="rs-login-card">
          <h1>Rustapi Studio</h1>
          <p className="rs-cell-muted">Loading…</p>
        </div>
      </div>
    );
  }

  if (mode === "unreachable") {
    return (
      <div className="rs-login">
        <div className="rs-login-card">
          <h1>Rustapi Studio</h1>
          <p className="rs-cell-muted">Can't reach the API.</p>
          <button className="rs-btn rs-btn--primary" onClick={probe}>
            Retry
          </button>
        </div>
      </div>
    );
  }

  const isSetup = mode === "setup";

  return (
    <div className="rs-login">
      <form className="rs-login-card" onSubmit={submit}>
        <h1>Rustapi Studio</h1>
        <p className="rs-cell-muted">
          {isSetup ? "Create the first admin account." : "Sign in to continue."}
        </p>
        <input
          className="rs-input"
          type="email"
          placeholder="Email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          autoFocus
          autoComplete="username"
        />
        <input
          className="rs-input"
          type="password"
          placeholder="Password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete={isSetup ? "new-password" : "current-password"}
        />
        {isSetup && (
          <input
            className="rs-input"
            type="password"
            placeholder="Confirm password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            autoComplete="new-password"
          />
        )}
        {error && <div className="rs-login-error">{error}</div>}
        <button
          className="rs-btn rs-btn--primary"
          type="submit"
          disabled={busy || !email || !password || (isSetup && !confirm)}
        >
          {busy ? "Please wait…" : isSetup ? "Create admin" : "Sign in"}
        </button>
      </form>
    </div>
  );
}
