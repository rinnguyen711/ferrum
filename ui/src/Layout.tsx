import { Outlet, useLocation, useMatch, useNavigate, useParams } from "react-router-dom";
import { Sidebar, SecondaryPanel, Topbar } from "./components/shell";
import { BuilderDraftProvider } from "./builder/BuilderDraftContext";
import { getClaims, clearToken } from "./auth";

export type Section = "dashboard" | "content" | "builder" | "settings" | "media" | "users";

function sectionFromPath(pathname: string): Section {
  if (pathname.startsWith("/media")) return "media";
  if (pathname.startsWith("/content")) return "content";
  if (pathname.startsWith("/builder")) return "builder";
  if (pathname.startsWith("/users")) return "users";
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

  const params = useParams<{ type?: string }>();
  const collection = params.type ?? "";

  const editorMatch = useMatch("/content/:type/:id");
  const showEditorBare = Boolean(editorMatch);

  const navigate = useNavigate();
  const email = getClaims()?.email ?? null;
  const onLogout = () => {
    clearToken();
    navigate("/login", { replace: true });
  };

  let crumbs: string[] | undefined;
  if (section === "dashboard") crumbs = ["Home"];
  else if (section === "content") crumbs = ["Content Manager", collection];
  else if (section === "builder") crumbs = ["Content-Type Builder", collection];
  else if (section === "settings") crumbs = ["Settings"];
  else if (section === "media") crumbs = ["Media Library"];
  else if (section === "users") crumbs = ["Users & Permissions"];

  return (
    <BuilderDraftProvider>
      <div className="rs-app">
        <Sidebar section={section} />
        <SecondaryPanel section={section} collection={collection} />
        <div className="rs-content">
          {!showEditorBare && (
            <Topbar crumbs={crumbs} dark={dark} onToggleDark={onToggleDark} email={email} onLogout={onLogout} />
          )}
          <div className={"rs-scroll" + (showEditorBare ? " rs-scroll--flush" : "")}>
            <Outlet />
          </div>
        </div>
      </div>
    </BuilderDraftProvider>
  );
}
