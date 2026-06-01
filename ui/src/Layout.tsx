import { Outlet, useLocation, useMatch, useParams } from "react-router-dom";
import { Sidebar, SecondaryPanel, Topbar } from "./components/shell";
import { RUSTAPI } from "./mock/data";

export type Section = "dashboard" | "content" | "builder" | "media" | "settings";

function sectionFromPath(pathname: string): Section {
  if (pathname.startsWith("/content")) return "content";
  if (pathname.startsWith("/builder")) return "builder";
  if (pathname.startsWith("/media")) return "media";
  if (pathname.startsWith("/settings")) return "settings";
  return "dashboard";
}

export function Layout({
  dark,
  onToggleDark,
}: {
  dark: boolean;
  onToggleDark: () => void;
}) {
  const location = useLocation();
  const section = sectionFromPath(location.pathname);

  // :type param shared by content + builder routes
  const params = useParams<{ type?: string }>();
  const collection = params.type ?? "article";

  // Entry editor route is /content/:type/:id — hide topbar + show full-bleed editor.
  const editorMatch = useMatch("/content/:type/:id");
  const showEditorBare = Boolean(editorMatch);

  const typeName = RUSTAPI.types[collection]?.plural ?? "";

  let crumbs: string[] | undefined;
  if (section === "dashboard") crumbs = ["Home"];
  else if (section === "content") crumbs = ["Content Manager", typeName];
  else if (section === "builder") crumbs = ["Content-Type Builder", typeName];
  else if (section === "media") crumbs = ["Media Library"];
  else if (section === "settings") crumbs = ["Settings", "API tokens"];

  return (
    <div className="rs-app">
      <Sidebar section={section} />
      <SecondaryPanel section={section} collection={collection} />
      <div className="rs-content">
        {!showEditorBare && (
          <Topbar crumbs={crumbs} dark={dark} onToggleDark={onToggleDark} />
        )}
        <div className={"rs-scroll" + (showEditorBare ? " rs-scroll--flush" : "")}>
          <Outlet />
        </div>
      </div>
    </div>
  );
}
