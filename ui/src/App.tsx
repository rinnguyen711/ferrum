import { useEffect, useState, type ReactNode } from "react";
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router-dom";
import { Layout } from "./Layout";
import { Dashboard } from "./screens/Dashboard";
import { SchemaEditor } from "./builder/SchemaEditor";
import { Settings } from "./screens/Settings";
import { ContentList } from "./screens/ContentList";
import { EntryEditor } from "./screens/EntryEditor";
import { Login } from "./screens/Login";
import { MediaLibrary } from "./screens/MediaLibrary";
import { Users } from "./screens/Users";
import { UserEditor } from "./screens/UserEditor";
import { Roles } from "./screens/Roles";
import { RoleEditor } from "./screens/RoleEditor";
import { MediaSettings } from "./screens/MediaSettings";
import { ComponentEditor } from "./screens/ComponentEditor";
import { SingleTypeEdit } from "./screens/SingleTypeEdit";
import { ApiTokens, TokenEditor, TokenDetail } from "./screens/ApiTokens";
import { Webhooks } from "./screens/Webhooks";
import { AuditLog } from "./screens/AuditLog";
import { WebhookEditor } from "./screens/WebhookEditor";
import { WebhookDetail } from "./screens/WebhookDetail";
import { getToken, clearToken } from "./auth";
import { setAuthErrorHandler } from "./api/client";
import { useResource } from "./hooks/useResource";
import { listContentTypes } from "./api/endpoints";

const ACCENT = "#D14D2B";
const DENSITY = "comfortable";
const UI_FONT = "IBM Plex Sans";
const DARK_KEY = "rustapi:dark";

function loadDark(): boolean {
  try {
    const v = localStorage.getItem(DARK_KEY);
    if (v != null) return v === "1";
    return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  } catch {
    return false;
  }
}

/** Landing for bare /content and /builder: redirect to the first content type
 * (matching the SecondaryPanel order). Falls back to a prompt when there are
 * no types yet, and shows nothing while the list is loading. */
function PickType({ kind }: { kind: "content" | "builder" }) {
  const base = kind === "builder" ? "/builder" : "/content";
  const { data, loading, error } = useResource(() => listContentTypes(), [base]);

  if (data && data.length > 0) {
    return <Navigate to={`${base}/${data[0].name}`} replace />;
  }
  if (loading) return <div className="rs-empty">Loading…</div>;

  const verb = kind === "builder" ? "inspect its schema" : "browse its entries";
  if (error) return <div className="rs-empty">Select a content type to {verb}.</div>;
  return <div className="rs-empty">No content types yet.</div>;
}

function RequireAuth({ children }: { children: ReactNode }) {
  const location = useLocation();
  if (!getToken()) {
    return <Navigate to="/login" replace state={{ from: location.pathname }} />;
  }
  return <>{children}</>;
}

/** Registers the global 401 handler once, inside the router context. */
function AuthErrorBridge() {
  const navigate = useNavigate();
  useEffect(() => {
    setAuthErrorHandler(() => {
      clearToken();
      navigate("/login", { replace: true });
    });
  }, [navigate]);
  return null;
}

export default function App() {
  const [dark, setDark] = useState<boolean>(loadDark);

  useEffect(() => {
    const r = document.documentElement;
    r.style.setProperty("--accent", ACCENT);
    r.setAttribute("data-density", DENSITY);
    r.style.setProperty("--ui-font", `"${UI_FONT}", system-ui, sans-serif`);
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
    try {
      localStorage.setItem(DARK_KEY, dark ? "1" : "0");
    } catch {
      // ignore quota / privacy-mode errors
    }
  }, [dark]);

  return (
    <BrowserRouter basename={import.meta.env.BASE_URL.replace(/\/$/, "") || "/"}>
      <AuthErrorBridge />
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route
          element={
            <RequireAuth>
              <Layout dark={dark} onToggleDark={() => setDark((d) => !d)} />
            </RequireAuth>
          }
        >
          <Route index element={<Dashboard />} />
          <Route path="content" element={<PickType kind="content" />} />
          <Route path="content/:type" element={<ContentList />} />
          <Route path="content/:type/:id" element={<EntryEditor />} />
          <Route path="content/single/:type" element={<SingleTypeEdit />} />
          <Route path="builder" element={<PickType kind="builder" />} />
          <Route path="builder/new" element={<SchemaEditor />} />
          <Route path="builder/:type" element={<SchemaEditor />} />
          <Route path="builder/components/:uid" element={<ComponentEditor />} />
          <Route path="settings" element={<Settings />} />
          <Route path="settings/api-tokens" element={<ApiTokens />} />
          <Route path="settings/api-tokens/new" element={<TokenEditor />} />
          <Route path="settings/api-tokens/:id" element={<TokenDetail />} />
          <Route path="settings/webhooks" element={<Webhooks />} />
          <Route path="settings/webhooks/new" element={<WebhookEditor />} />
          <Route path="settings/webhooks/:id" element={<WebhookDetail />} />
          <Route path="settings/audit" element={<AuditLog />} />
          <Route path="settings/media" element={<MediaSettings />} />
          <Route path="users" element={<Users />} />
          <Route path="users/new" element={<UserEditor />} />
          <Route path="users/:id" element={<UserEditor />} />
          <Route path="roles" element={<Roles />} />
          <Route path="roles/new" element={<RoleEditor />} />
          <Route path="roles/:key" element={<RoleEditor />} />
          <Route path="media" element={<MediaLibrary />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
