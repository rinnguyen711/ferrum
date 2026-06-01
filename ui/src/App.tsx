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
import { getKey, clearKey } from "./auth";
import { setAuthErrorHandler } from "./api/client";

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

/** Landing for bare /content and /builder: the type chooser lives in the
 * SecondaryPanel, so the main area just prompts the user to pick one. */
function PickType({ kind }: { kind: "content" | "builder" }) {
  const verb = kind === "builder" ? "inspect its schema" : "browse its entries";
  return <div className="rs-empty">Select a content type to {verb}.</div>;
}

function RequireAuth({ children }: { children: ReactNode }) {
  const location = useLocation();
  if (!getKey()) {
    return <Navigate to="/login" replace state={{ from: location.pathname }} />;
  }
  return <>{children}</>;
}

/** Registers the global 401 handler once, inside the router context. */
function AuthErrorBridge() {
  const navigate = useNavigate();
  useEffect(() => {
    setAuthErrorHandler(() => {
      clearKey();
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
          <Route path="builder" element={<PickType kind="builder" />} />
          <Route path="builder/new" element={<SchemaEditor />} />
          <Route path="builder/:type" element={<SchemaEditor />} />
          <Route path="settings" element={<Settings />} />
          <Route path="media" element={<Navigate to="/" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
