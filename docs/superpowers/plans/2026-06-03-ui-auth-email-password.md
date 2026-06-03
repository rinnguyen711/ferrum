# UI Auth (Email/Password + JWT) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the admin studio UI from `x-api-key` auth to email/password + JWT, with a first-run create-admin flow and a logout control.

**Architecture:** The UI auth seam is `auth.ts` (token store) + `client.ts` (header) + `endpoints.ts` (calls) + `Login.tsx` (form) + `App.tsx` (guard). Swap key→JWT semantics across them; the login screen auto-detects first-run via a new public `GET /auth/setup` → `{ setup_required }`. Token lives in localStorage; the Topbar shows the decoded email and a logout button.

**Tech Stack:** React 18 + TypeScript + react-router-dom (UI, no test framework — typecheck only); Rust/Axum + sqlx (backend addition); cargo test for the one new backend test.

**Spec:** `docs/superpowers/specs/2026-06-03-ui-auth-email-password-design.md`

**Verification commands:**
- UI typecheck: `cd ui && pnpm typecheck`
- Backend tests: `cargo test -p rustapi-http` (unit) and `cargo test -p rustapi --test integration_auth` (integration, needs Docker)

---

## File Structure

- `crates/http/src/auth/users.rs` — **modify**: add `any_users(pool) -> bool`.
- `crates/http/src/auth/handlers.rs` — **modify**: add `setup_status` handler + `SetupStatus` response.
- `crates/http/src/auth/mod.rs` — **modify**: add `GET /auth/setup` to `public_router()`.
- `crates/bin/tests/integration_auth.rs` — **modify**: test setup-status before/after setup.
- `ui/src/auth.ts` — **rewrite**: token store (`getToken`/`setToken`/`clearToken`) + `getClaims()`.
- `ui/src/api/client.ts` — **modify**: `Authorization: Bearer`; 401 message; import `getToken`.
- `ui/src/api/endpoints.ts` — **modify**: remove `checkAuth`; add `login`, `setup`, `fetchSetupStatus`.
- `ui/src/api/types.ts` — **modify**: add `LoginResponse`, `SetupStatus` types.
- `ui/src/screens/Login.tsx` — **rewrite**: two-mode email/password form.
- `ui/src/App.tsx` — **modify**: `RequireAuth` + bridge use token fns.
- `ui/src/Layout.tsx` — **modify**: pass logout handler to Topbar.
- `ui/src/components/shell.tsx` — **modify**: Topbar shows email + logout button.

---

## Task 1: Backend — `any_users` store helper

**Files:**
- Modify: `crates/http/src/auth/users.rs`

- [ ] **Step 1: Add the helper**

In `crates/http/src/auth/users.rs`, after the `UserRow` struct (before `find_by_email`), add:

```rust
/// True if any user exists. Backs the public setup-status endpoint.
pub async fn any_users(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let (exists,): (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM _users)")
        .fetch_one(pool)
        .await?;
    Ok(exists)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p rustapi-http`
Expected: compiles, no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/auth/users.rs
git commit -m "feat(http): any_users store helper"
```

---

## Task 2: Backend — `GET /auth/setup` status endpoint

**Files:**
- Modify: `crates/http/src/auth/handlers.rs`
- Modify: `crates/http/src/auth/mod.rs`

- [ ] **Step 1: Add the handler + response type**

In `crates/http/src/auth/handlers.rs`, add the response struct next to `UserView` (after it):

```rust
#[derive(Serialize)]
pub struct SetupStatus {
    pub setup_required: bool,
}
```

Then add the handler after `me` (before the `DUMMY_HASH` const):

```rust
/// GET /auth/setup — public. Reports whether first-run setup is still open.
pub async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    let exists = users::any_users(&state.pool).await.map_err(internal)?;
    Ok(Json(SetupStatus {
        setup_required: !exists,
    }))
}
```

- [ ] **Step 2: Wire the route**

In `crates/http/src/auth/mod.rs`, change `public_router()` to add a GET on the same path:

```rust
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/setup", get(handlers::setup_status).post(handlers::setup))
        .route("/auth/login", post(handlers::login))
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p rustapi-http`
Expected: compiles. (`get` is already imported in `mod.rs`.)

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/auth/handlers.rs crates/http/src/auth/mod.rs
git commit -m "feat(http): GET /auth/setup status endpoint"
```

---

## Task 3: Backend — integration test for setup status

**Files:**
- Modify: `crates/bin/tests/integration_auth.rs`

- [ ] **Step 1: Add the test**

In `crates/bin/tests/integration_auth.rs`, add after `setup_is_self_closing`:

```rust
#[tokio::test]
async fn setup_status_flips_after_setup() {
    // spawn() runs setup once, so by the time we get the app, setup is closed.
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .get(app.url("/auth/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["setup_required"], false);
}
```

> Note: a fresh-DB `setup_required == true` assertion would require a TestApp
> variant that skips the setup-in-spawn step. `spawn()` always seeds the admin,
> so we assert the post-setup state here; the unit-level empty case is covered
> by `any_users` returning false→true implicitly through this and the existing
> `setup_is_self_closing` test.

- [ ] **Step 2: Run the test**

Run: `cargo test -p rustapi --test integration_auth setup_status_flips_after_setup`
Expected: PASS (needs Docker).

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests/integration_auth.rs
git commit -m "test(bin): GET /auth/setup status integration test"
```

---

## Task 4: UI — token store (`auth.ts`)

**Files:**
- Rewrite: `ui/src/auth.ts`

- [ ] **Step 1: Rewrite the module**

Replace the entire contents of `ui/src/auth.ts` with:

```ts
const TOKEN = "rustapi:token";

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN);
  } catch {
    return null;
  }
}

export function setToken(token: string): void {
  try {
    localStorage.setItem(TOKEN, token);
  } catch {
    // ignore quota / privacy-mode errors
  }
}

export function clearToken(): void {
  try {
    localStorage.removeItem(TOKEN);
  } catch {
    // ignore
  }
}

/** JWT claims we care about for display. Never trusted for authz — the server
 * is authoritative; this only drives UI labels. */
export interface Claims {
  sub: string;
  email: string;
  roles: string[];
}

/** Decode (not verify) the JWT payload for display. Returns null if absent or
 * malformed. */
export function getClaims(): Claims | null {
  const token = getToken();
  if (!token) return null;
  const parts = token.split(".");
  if (parts.length !== 3) return null;
  try {
    const json = atob(parts[1].replace(/-/g, "+").replace(/_/g, "/"));
    const obj = JSON.parse(json) as Partial<Claims>;
    if (typeof obj.email !== "string" || !Array.isArray(obj.roles)) return null;
    return { sub: String(obj.sub ?? ""), email: obj.email, roles: obj.roles };
  } catch {
    return null;
  }
}
```

- [ ] **Step 2: Verify typecheck (will fail on consumers, expected)**

Run: `cd ui && pnpm typecheck`
Expected: FAILS — `client.ts`, `App.tsx`, `endpoints.ts`, `Login.tsx` still import the removed `getKey`/`setKey`/`clearKey`. These are fixed in Tasks 5–9. Do not commit yet; this task's commit folds in after Task 9 makes typecheck pass, OR commit now and accept a transient red typecheck (the plan fixes it within the same branch).

> Commit this file together with Tasks 5–9 (see Task 9 Step 4) so the branch
> never has a committed broken typecheck.

---

## Task 5: UI — API types

**Files:**
- Modify: `ui/src/api/types.ts`

- [ ] **Step 1: Add the response types**

In `ui/src/api/types.ts`, append:

```ts
export interface LoginResponse {
  token: string;
  expires_at: number;
}

export interface SetupStatus {
  setup_required: boolean;
}
```

- [ ] **Step 2: Verify the file parses**

Run: `cd ui && pnpm typecheck`
Expected: still FAILS on the auth consumers (Tasks 6–9), but no NEW errors in `types.ts`. Proceed.

---

## Task 6: UI — client.ts bearer header

**Files:**
- Modify: `ui/src/api/client.ts`

- [ ] **Step 1: Swap the import**

In `ui/src/api/client.ts`, change line 1:

```ts
import { getToken } from "../auth";
```

- [ ] **Step 2: Swap the header logic**

Replace the body of `apiFetch` from the `const key` line through the headers block. Change:

```ts
  const key = opts.key ?? getToken();
  const headers: Record<string, string> = { Accept: "application/json" };
  if (key) headers["x-api-key"] = key;
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";
```

to:

```ts
  const token = opts.token ?? getToken();
  const headers: Record<string, string> = { Accept: "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";
```

- [ ] **Step 3: Rename the `FetchOpts.key` field**

In the `FetchOpts` interface, change:

```ts
  /** When set, sends this key instead of the stored one (login probe). */
  key?: string;
```

to:

```ts
  /** When set, sends this token instead of the stored one. */
  token?: string;
```

- [ ] **Step 4: Update the 401 block**

Change:

```ts
  if (resp.status === 401) {
    const err = new AuthError("Invalid or missing admin key.");
    // Only fire the global handler for stored-key requests, not login probes.
    if (opts.key === undefined && onAuthError) onAuthError();
    throw err;
  }
```

to:

```ts
  if (resp.status === 401) {
    const err = new AuthError("Invalid or missing credentials.");
    // Only fire the global handler for stored-token requests, not explicit ones.
    if (opts.token === undefined && onAuthError) onAuthError();
    throw err;
  }
```

- [ ] **Step 5: Verify no new client.ts errors**

Run: `cd ui && pnpm typecheck`
Expected: still fails on `endpoints.ts`/`Login.tsx`/`App.tsx`, but `client.ts` itself clean. Proceed.

---

## Task 7: UI — endpoints (login, setup, status)

**Files:**
- Modify: `ui/src/api/endpoints.ts`

- [ ] **Step 1: Update imports**

In `ui/src/api/endpoints.ts`, change line 2 to include the new types:

```ts
import type {
  ContentType,
  Entry,
  Health,
  ListResponse,
  LoginResponse,
  NewContentType,
  PatchContentType,
  SetupStatus,
} from "./types";
```

> If any of those names are not currently imported (e.g. `Entry`, `ListResponse`),
> keep the ones already present and add `LoginResponse` + `SetupStatus`. Match the
> existing import list; only add the two new names.

- [ ] **Step 2: Replace `checkAuth` with the auth calls**

Delete the `checkAuth` function (the `/** Probe the gated... */` block through its close) and replace with:

```ts
/** First-run check: does the system still need an admin created? */
export function fetchSetupStatus(): Promise<SetupStatus> {
  return apiFetch<SetupStatus>("/auth/setup");
}

/** Create the first admin. Only succeeds on an empty system (else 409). */
export function setup(email: string, password: string): Promise<void> {
  return apiFetch<void>("/auth/setup", { method: "POST", body: { email, password } });
}

/** Exchange credentials for a JWT. */
export function login(email: string, password: string): Promise<LoginResponse> {
  return apiFetch<LoginResponse>("/auth/login", { method: "POST", body: { email, password } });
}
```

- [ ] **Step 3: Verify no new endpoints.ts errors**

Run: `cd ui && pnpm typecheck`
Expected: fails only on `Login.tsx` + `App.tsx` now. Proceed.

---

## Task 8: UI — Login screen (two-mode form)

**Files:**
- Rewrite: `ui/src/screens/Login.tsx`

- [ ] **Step 1: Rewrite the screen**

Replace the entire contents of `ui/src/screens/Login.tsx` with:

```tsx
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
```

- [ ] **Step 2: Verify Login.tsx typechecks**

Run: `cd ui && pnpm typecheck`
Expected: fails only on `App.tsx` now (still imports `getKey`/`clearKey`). Proceed.

---

## Task 9: UI — App guard + auth bridge

**Files:**
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: Swap the import**

In `ui/src/App.tsx`, change:

```ts
import { getKey, clearKey } from "./auth";
```

to:

```ts
import { getToken, clearToken } from "./auth";
```

- [ ] **Step 2: Update `RequireAuth`**

Change `if (!getKey()) {` to:

```ts
  if (!getToken()) {
```

- [ ] **Step 3: Update `AuthErrorBridge`**

Change `clearKey();` to:

```ts
      clearToken();
```

- [ ] **Step 4: Verify full UI typecheck passes**

Run: `cd ui && pnpm typecheck`
Expected: PASS — all `getKey`/`setKey`/`clearKey`/`checkAuth` references resolved.

- [ ] **Step 5: Commit the UI auth core (Tasks 4–9)**

```bash
git add ui/src/auth.ts ui/src/api/client.ts ui/src/api/endpoints.ts ui/src/api/types.ts ui/src/screens/Login.tsx ui/src/App.tsx
git commit -m "feat(ui): email/password + JWT auth, first-run create-admin flow"
```

---

## Task 10: UI — Topbar email + logout

**Files:**
- Modify: `ui/src/components/shell.tsx`
- Modify: `ui/src/Layout.tsx`

- [ ] **Step 1: Inspect the Layout → Topbar wiring**

Run: `grep -n "Topbar\|onToggleDark\|dark" ui/src/Layout.tsx`
Expected: shows how `Topbar` is rendered (around line 45) and how `dark`/`onToggleDark` props flow. The logout handler will be passed the same way.

- [ ] **Step 2: Add an `onLogout` prop to Topbar and render email + logout**

In `ui/src/components/shell.tsx`, extend the `Topbar` signature. Change:

```tsx
export function Topbar({
  title,
  crumbs,
  right,
  dark,
  onToggleDark,
}: {
  title?: string;
  crumbs?: string[];
  right?: ReactNode;
  dark: boolean;
  onToggleDark: () => void;
}) {
```

to:

```tsx
export function Topbar({
  title,
  crumbs,
  right,
  dark,
  onToggleDark,
  email,
  onLogout,
}: {
  title?: string;
  crumbs?: string[];
  right?: ReactNode;
  dark: boolean;
  onToggleDark: () => void;
  email?: string | null;
  onLogout?: () => void;
}) {
```

- [ ] **Step 3: Replace the user block**

Change the `rs-topbar-user` block:

```tsx
        <div className="rs-topbar-user">
          <Avatar name="Admin" initials="AD" color="#52525B" size={28} />
          <div className="rs-topbar-user-meta">
            <strong>Admin</strong>
            <span>API key</span>
          </div>
          <Icons.chevDown size={15} />
        </div>
```

to:

```tsx
        <div className="rs-topbar-user">
          <Avatar
            name={email ?? "Admin"}
            initials={(email ?? "AD").slice(0, 2).toUpperCase()}
            color="#52525B"
            size={28}
          />
          <div className="rs-topbar-user-meta">
            <strong>{email ?? "Admin"}</strong>
            <span>Signed in</span>
          </div>
          <button
            className="rs-icon-btn"
            data-tip="Sign out"
            onClick={onLogout}
            aria-label="Sign out"
          >
            <Icons.arrowLeft size={18} />
          </button>
        </div>
```

- [ ] **Step 4: Pass email + logout from Layout**

In `ui/src/Layout.tsx`, add imports at the top (match the existing import style):

```ts
import { useNavigate } from "react-router-dom";
import { getClaims, clearToken } from "./auth";
```

Inside the `Layout` component body, before the `return`, add:

```ts
  const navigate = useNavigate();
  const email = getClaims()?.email ?? null;
  const onLogout = () => {
    clearToken();
    navigate("/login", { replace: true });
  };
```

Then update the `<Topbar ... />` render to pass the new props:

```tsx
            <Topbar crumbs={crumbs} dark={dark} onToggleDark={onToggleDark} email={email} onLogout={onLogout} />
```

> If `Topbar` is rendered in more than one place in `Layout.tsx`, add
> `email={email}` and `onLogout={onLogout}` to each.

- [ ] **Step 5: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 6: Build the UI bundle**

Run: `cd ui && pnpm build`
Expected: `tsc -b` + `vite build` succeed, bundle emitted to `ui/dist`.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/shell.tsx ui/src/Layout.tsx
git commit -m "feat(ui): Topbar shows email + sign-out button"
```

---

## Task 11: Manual smoke verification

**Files:** none (verification only)

- [ ] **Step 1: Start a backend with an empty DB**

Run (in one terminal):
```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/rustapi_uiauth
export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
export RUSTAPI_STUDIO_DIR=$PWD/ui/dist
createdb -h localhost -U postgres rustapi_uiauth 2>/dev/null || true
cargo run -p rustapi
```

> If no local Postgres is available, use the docker-compose stack instead:
> `RUSTAPI_JWT_SECRET=$(openssl rand -hex 32) docker compose up --build`
> and point a browser at the compose port. Skip if no environment is available
> and note that the smoke was not run.

- [ ] **Step 2: Walk the flow in a browser**

Open `http://localhost:8080/studio`. Verify in order:
1. Redirects to `/login`, which shows the **Create admin** form (empty DB).
2. Submit email + password (≥ 8 chars) + matching confirm → lands on the dashboard.
3. Topbar shows the email + a sign-out button.
4. Click sign-out → back to `/login`, now showing the **Sign in** form.
5. Sign in with the same credentials → dashboard again.
6. Reload the page → still authenticated (token persisted).
7. In devtools, delete the `rustapi:token` localStorage key, then trigger any
   API call (navigate to Content) → redirected to `/login`.

- [ ] **Step 3: Record the result**

No commit. Report which steps passed. If any failed, stop and report the failure
rather than proceeding to branch completion.

---

## Self-Review Notes

**Spec coverage:**
- Auto-detect setup mode → Tasks 2 (backend status), 8 (Login two-mode). ✓
- `GET /auth/setup` → `{ setup_required }` → Tasks 1, 2, 3. ✓
- localStorage token store → Task 4. ✓
- Logout in Topbar + email display → Task 10. ✓
- JWT payload decode for email → Task 4 (`getClaims`). ✓
- Bearer header swap → Task 6. ✓
- `login`/`setup`/`fetchSetupStatus` endpoints → Task 7. ✓
- RequireAuth/bridge on token → Task 9. ✓
- Setup→login chain, 409 flip, 401 + 422 messages → Task 8. ✓
- Testing (typecheck + cargo test + manual smoke) → Tasks 3, 9, 10, 11. ✓

**Known sequencing note:** Tasks 4–9 each leave `pnpm typecheck` red until Task 9
closes the loop; they are committed together at Task 9 Step 5 so no broken
typecheck is ever committed. Backend Tasks 1–3 are independently green and
committed separately.

**Type consistency:** `getToken`/`setToken`/`clearToken`/`getClaims` (Task 4)
are the exact names used in Tasks 6, 8, 9, 10. `LoginResponse`/`SetupStatus`
(Task 5) match usage in Tasks 7, 8. `FetchOpts.token` (Task 6) matches the
`opts.token` reads in the same task. `onLogout`/`email` Topbar props (Task 10
Step 2) match the render in Step 4.
