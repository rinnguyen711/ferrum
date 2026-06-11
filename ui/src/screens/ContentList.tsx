import { useEffect, useMemo, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { getToken } from "../auth";
import { Avatar, StatusBadge } from "../components/shell";
import { Checkbox, LoadingState, EmptyState } from "../components/ui";
import { FieldsMenu, type ColumnDef } from "./FieldsMenu";
import { useResource } from "../hooks/useResource";
import { getContentType, listContentTypes, listEntries } from "../api/endpoints";
import type { ContentType, Entry, Field } from "../api/types";
import { draftPublishEnabled, relationMeta } from "../api/types";
import { relTime, relationLabel, shortId, initials, AVATAR_NEUTRAL } from "../util";

const STATUS_TABS: [string, string][] = [
  ["all", "All"],
  ["published", "Published"],
  ["review", "In review"],
  ["draft", "Draft"],
];

const PUBLISH_TABS: [("published" | "draft" | "all"), string][] = [
  ["published", "Published"],
  ["draft", "Draft"],
  ["all", "All"],
];

export function ContentList() {
  const { type = "" } = useParams<{ type: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  type FlashState = { flash?: string; flashId?: string } | null;
  const locState = location.state as FlashState;
  const [flash, setFlash] = useState<{ verb: string; id: string } | null>(() =>
    locState?.flash && locState?.flashId ? { verb: locState.flash, id: locState.flashId } : null
  );

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), 5000);
    window.history.replaceState({ ...window.history.state, usr: {} }, "");
    return () => clearTimeout(t);
  }, [flash]);

  const schema = useResource(() => getContentType(type), [type]);
  const allTypes = useResource(() => listContentTypes(), []);

  const ct = schema.data;
  const dp = ct ? draftPublishEnabled(ct) : false;
  const populate = ct
    ? ct.fields.filter((f) => f.kind === "relation").map((f) => f.name).join(",")
    : "";

  const [publishFilter, setPublishFilter] = useState<"published" | "draft" | "all">("all");

  const entries = useResource(
    () => listEntries(type, { populate: populate || undefined, pageSize: 100, status: dp ? publishFilter : undefined }),
    [type, populate, dp, publishFilter],
  );

  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [selected, setSelected] = useState<string[]>([]);
  const [fieldsOpen, setFieldsOpen] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importResult, setImportResult] = useState<{
    inserted: number;
    updated: number;
    errors: { row: number; message: string }[];
  } | null>(null);

  const handleImport = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setImporting(true);
    setImportResult(null);
    try {
      const form = new FormData();
      form.append('file', file);
      const token = getToken();
      const resp = await fetch(`/admin/content-types/${type}/entries/import`, {
        method: 'POST',
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        body: form,
      });
      if (!resp.ok) {
        const text = await resp.text();
        setImportResult({ inserted: 0, updated: 0, errors: [{ row: 0, message: text || `HTTP ${resp.status}` }] });
        return;
      }
      const data = await resp.json();
      setImportResult(data);
      entries.refetch();
    } catch (err) {
      setImportResult({ inserted: 0, updated: 0, errors: [{ row: 0, message: String(err) }] });
    } finally {
      setImporting(false);
      e.target.value = '';
    }
  };

  const handleExport = async () => {
    if (selected.length === 0) return;
    const params = `ids=${selected.map(encodeURIComponent).join(',')}`;
    const token = getToken();
    try {
      const resp = await fetch(
        `/admin/content-types/${type}/entries/export?${params}`,
        { headers: token ? { Authorization: `Bearer ${token}` } : {} }
      );
      if (!resp.ok) return; // silent on error — export is best-effort
      const blob = await resp.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${type}.csv`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch {
      // network error — ignore
    }
  };

  const colKey = `rs-cols:${type}`;
  const [hidden, setHidden] = useState<Record<string, boolean>>(() => {
    try {
      return JSON.parse(localStorage.getItem(colKey) || "{}");
    } catch {
      return {};
    }
  });
  const setHiddenPersist = (next: Record<string, boolean>) => {
    setHidden(next);
    localStorage.setItem(colKey, JSON.stringify(next));
  };
  const toggleCol = (key: string) =>
    setHiddenPersist({ ...hidden, [key]: !hidden[key] });
  const resetCols = () => {
    setHidden({});
    localStorage.removeItem(colKey);
  };

  const hasStatus = !!ct?.fields.some((f) => f.name === "status" && f.kind === "enum");
  const titleField = ct?.fields.find((f) => ["title", "name"].includes(f.name))?.name;

  const rows = entries.data?.data ?? [];
  const filtered = useMemo(() => {
    return rows.filter((e) => {
      if (hasStatus && statusFilter !== "all" && e["status"] !== statusFilter) return false;
      if (query && titleField) {
        const t = String(e[titleField] ?? "").toLowerCase();
        if (!t.includes(query.toLowerCase())) return false;
      }
      return true;
    });
  }, [rows, statusFilter, query, hasStatus, titleField]);

  if (schema.loading || entries.loading) return <LoadingState />;
  if (schema.error)
    return (
      <EmptyState>
        Couldn't load type "{type}".{" "}
        <button className="rs-link-btn" onClick={schema.refetch}>Retry</button>
      </EmptyState>
    );
  if (entries.error)
    return (
      <EmptyState>
        {entries.error.message}{" "}
        <button className="rs-link-btn" onClick={entries.refetch}>Retry</button>
      </EmptyState>
    );
  if (!ct || !entries.data) return <EmptyState>Unknown content type.</EmptyState>;

  const allColumns: ColumnDef[] = [
    { key: "id", label: "ID" },
    ...ct.fields.map((f) => ({ key: f.name, label: f.name })),
    { key: "updated", label: "Updated" },
  ];
  const lockedKey = titleField; // title column always shown
  const colVisible = (key: string) => key === lockedKey || !hidden[key];
  const cols = ct.fields.filter((f) => colVisible(f.name));
  const visibleMap = Object.fromEntries(
    allColumns.map((c) => [c.key, colVisible(c.key)]),
  );
  const total = entries.data.meta.total;
  const statusCount = (s: string) =>
    s === "all" ? rows.length : rows.filter((e) => e["status"] === s).length;

  const targetSchema = (f: Field): ContentType | undefined => {
    const m = relationMeta(f);
    return m ? allTypes.data?.find((t) => t.name === m.target) : undefined;
  };

  const allOn = filtered.length > 0 && selected.length === filtered.length;
  const toggleAll = () => setSelected(allOn ? [] : filtered.map((e) => e.id));
  const toggle = (id: string) =>
    setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));

  const renderCell = (entry: Entry, f: Field): React.ReactNode => {
    const v = entry[f.name];
    if (v == null || v === "") return <span className="rs-cell-muted">—</span>;
    if (f.name === "status" && f.kind === "enum") {
      return <StatusBadge status={String(v) as "draft" | "review" | "published"} />;
    }
    switch (f.kind) {
      case "relation": {
        const obj = typeof v === "object" ? (v as Entry) : null;
        const label = relationLabel(v, targetSchema(f));
        if (obj && "name" in obj) {
          return (
            <span className="rs-cell-author">
              <Avatar name={String(label)} initials={initials(String(label))} color={AVATAR_NEUTRAL} size={22} />
              {label}
            </span>
          );
        }
        return label;
      }
      case "datetime":
        return relTime(typeof v === "string" ? v : null);
      case "boolean":
        return v ? <Icons.check size={14} /> : <span className="rs-cell-muted">—</span>;
      case "enum":
        return <span className="rs-type-pill">{String(v)}</span>;
      case "json":
        return <code className="rs-mono">{JSON.stringify(v)}</code>;
      default:
        return String(v);
    }
  };

  return (
    <div className="rs-cm">
      {flash && (
        <div className="rs-cm-flash">
          Object{" "}
          <Link className="rs-cm-flash-link" to={`/content/${type}/${flash.id}`}>
            #{shortId(flash.id)}
          </Link>{" "}
          has been {flash.verb} successfully.
        </div>
      )}
      <div className="rs-cm-head">
        <div>
          <h1>{ct.display_name}</h1>
          <p className="rs-cm-sub">{total} entries</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => navigate(`/content/${type}/new`)}>
          <Icons.plus size={16} /> Create new entry
        </button>
      </div>

      {dp ? (
        <div className="rs-cm-tabs">
          {PUBLISH_TABS.map(([k, l]) => (
            <button
              key={k}
              className={"rs-tab" + (publishFilter === k ? " is-active" : "")}
              onClick={() => setPublishFilter(k)}
            >
              {l}
            </button>
          ))}
        </div>
      ) : hasStatus && (
        <div className="rs-cm-tabs">
          {STATUS_TABS.map(([k, l]) => (
            <button
              key={k}
              className={"rs-tab" + (statusFilter === k ? " is-active" : "")}
              onClick={() => setStatusFilter(k)}
            >
              {l} <span className="rs-tab-count">{statusCount(k)}</span>
            </button>
          ))}
        </div>
      )}

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input
            placeholder={`Search ${ct.display_name.toLowerCase()}`}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon">
          <Icons.filter size={15} /> Filters
        </button>
        <div className="rs-spacer" />
        <div className="rs-pop-anchor">
          <button
            className={"rs-btn rs-btn--ghost" + (fieldsOpen ? " is-active" : "")}
            onClick={() => setFieldsOpen((o) => !o)}
          >
            <Icons.layers size={15} /> Fields
          </button>
          {fieldsOpen && (
            <FieldsMenu
              columns={allColumns}
              visible={visibleMap}
              lockedKey={lockedKey}
              onToggle={toggleCol}
              onReset={resetCols}
              onClose={() => setFieldsOpen(false)}
            />
          )}
        </div>
        <>
          <input
            id="import-csv-input"
            type="file"
            accept=".csv"
            style={{ display: 'none' }}
            onChange={handleImport}
          />
          <button
            className="rs-btn rs-btn--ghost"
            onClick={() => document.getElementById('import-csv-input')?.click()}
            disabled={importing}
          >
            <Icons.upload size={15} /> {importing ? 'Importing…' : 'Import CSV'}
          </button>
        </>
      </div>

      {importResult && (
        <div className={`rs-notice ${importResult.errors.length > 0 ? 'rs-notice--warn' : 'rs-notice--ok'}`}>
          <span>
            Imported {importResult.inserted + importResult.updated} rows
            ({importResult.inserted} new, {importResult.updated} updated
            {importResult.errors.length > 0 && `, ${importResult.errors.length} errors`})
          </span>
          {importResult.errors.length > 0 && (
            <ul>
              {importResult.errors.map((e) => (
                <li key={e.row}>Row {e.row}: {e.message}</li>
              ))}
            </ul>
          )}
          <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setImportResult(null)}>
            Dismiss
          </button>
        </div>
      )}

      {selected.length > 0 && (
        <div className="rs-bulkbar">
          <span><strong>{selected.length}</strong> selected</span>
          <div className="rs-bulkbar-actions">
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={handleExport}>
              <Icons.doc size={14} /> Export CSV
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm" data-placeholder title="Coming soon">
              <Icons.eye size={14} /> Publish
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm rs-danger" data-placeholder title="Coming soon">
              <Icons.trash size={14} /> Delete
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setSelected([])}>Clear</button>
          </div>
        </div>
      )}

      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th className="rs-col-check">
                <Checkbox checked={allOn} onChange={toggleAll} />
              </th>
              {colVisible("id") && <th className="rs-col-id">ID</th>}
              {dp && <th>Status</th>}
              {cols.map((f) => <th key={f.name}>{f.name}</th>)}
              {colVisible("updated") && <th>Updated</th>}
            </tr>
          </thead>
          <tbody>
            {filtered.map((e) => (
              <tr
                key={e.id}
                className={selected.includes(e.id) ? "is-selected" : ""}
                onClick={() => navigate(`/content/${type}/${e.id}`)}
              >
                <td className="rs-col-check" onClick={(ev) => ev.stopPropagation()}>
                  <Checkbox checked={selected.includes(e.id)} onChange={() => toggle(e.id)} />
                </td>
                {colVisible("id") && <td className="rs-col-id rs-mono">{shortId(e.id)}</td>}
                {dp && (
                  <td>
                    <StatusBadge status={e.published_at ? "published" : "draft"} />
                  </td>
                )}
                {cols.map((f) => <td key={f.name}>{renderCell(e, f)}</td>)}
                {colVisible("updated") && <td className="rs-cell-muted">{relTime(e.updated_at)}</td>}
              </tr>
            ))}
          </tbody>
        </table>
        {filtered.length === 0 && <div className="rs-empty">No entries match.</div>}
      </div>

      <div className="rs-pager">
        <span className="rs-cell-muted">Showing {filtered.length} of {total}</span>
        <div className="rs-pager-ctrl">
          <button className="rs-page-btn is-active">1</button>
          <button className="rs-page-btn" data-placeholder disabled title="Coming soon">
            <Icons.chevRight size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}

