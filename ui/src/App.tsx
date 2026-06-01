import { useEffect, useState } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { Layout } from "./Layout";
import { Dashboard } from "./screens/Dashboard";
import { MediaLibrary } from "./screens/MediaLibrary";
import { ContentTypeBuilder } from "./screens/ContentTypeBuilder";
import { Settings } from "./screens/Settings";
import { ContentList } from "./screens/ContentList";
import { EntryEditor } from "./screens/EntryEditor";

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
      <Routes>
        <Route element={<Layout dark={dark} onToggleDark={() => setDark((d) => !d)} />}>
          <Route index element={<Dashboard />} />
          <Route path="content" element={<Navigate to="/content/article" replace />} />
          <Route path="content/:type" element={<ContentList />} />
          <Route path="content/:type/:id" element={<EntryEditor />} />
          <Route path="builder" element={<Navigate to="/builder/article" replace />} />
          <Route path="builder/:type" element={<ContentTypeBuilder />} />
          <Route path="media" element={<MediaLibrary />} />
          <Route path="settings" element={<Settings />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
