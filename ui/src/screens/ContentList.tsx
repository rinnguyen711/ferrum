import { useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { getToken } from "../auth";
import { Avatar, StatusBadge } from "../components/shell";
import { Checkbox, LoadingState, EmptyState } from "../components/ui";
import { FieldsMenu, type ColumnDef } from "./FieldsMenu";
import {
  FiltersMenu,
  filterableFields,
  isComplete,
  makeRule,
  serializeFilters,
  type FilterRule,
} from "./FiltersMenu";
import { useResource } from "../hooks/useResource";
import {
  deleteEntry, getContentType, listContentTypes, listEntries,
  publishEntry, unpublishEntry,
} from "../api/endpoints";
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
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(25);
  const [sortKey, setSortKey] = useState<"updated" | "title">("updated");

  const [filterRules, setFilterRules] = useState<FilterRule[]>([]);
  const [filtersOpen, setFiltersOpen] = useState(false);
  useEffect(() => {
    setFilterRules([]);
    setFiltersOpen(false);
    setQuery("");
    setStatusFilter("all");
    setSortKey("updated");
  }, [type]);

  const hasStatus = !!ct?.fields.some((f) => f.name === "status" && f.kind === "enum");
  const titleField = ct?.fields.find((f) => ["title", "name"].includes(f.name))?.name;

  // All server-side list shaping (user filters, search, enum-status tab)
  // funnels through one debounced pair list so typing doesn't refetch per
  // keystroke. JSON string is the stable dep representation.
  const allPairs: [string, string][] = ct ? serializeFilters(filterRules, ct) : [];
  if (query && titleField) allPairs.push([`filters[${titleField}][$containsi]`, query]);
  if (hasStatus && statusFilter !== "all") allPairs.push(["filters[status][$eq]", statusFilter]);
  const pairsJson = JSON.stringify(allPairs);
  const [debouncedPairs, setDebouncedPairs] = useState(pairsJson);
  useEffect(() => {
    const t = setTimeout(() => setDebouncedPairs(pairsJson), 300);
    return () => clearTimeout(t);
  }, [pairsJson]);
  const activeFilterCount = filterRules.filter(isComplete).length;

  const sort = sortKey === "title" && titleField ? `${titleField}:asc` : "updated_at:desc";

  const entries = useResource(
    () =>
      listEntries(type, {
        populate: populate || undefined,
        page,
        pageSize,
        sort,
        status: dp ? publishFilter : undefined,
        filters: JSON.parse(debouncedPairs) as [string, string][],
      }),
    [type, populate, dp, publishFilter, debouncedPairs, page, pageSize, sort],
  );

  const [selected, setSelected] = useState<string[]>([]);

  // Any query-shape change invalidates the current page and selection.
  useEffect(() => {
    setPage(1);
    setSelected([]);
  }, [type, debouncedPairs, publishFilter, pageSize, sort]);

  // Deleting the last rows of the final page can strand `page` past the
  // end — clamp it back once the new total arrives.
  useEffect(() => {
    const t = entries.data?.meta.total ?? 0;
    const pc = Math.max(1, Math.ceil(t / pageSize));
    if (page > pc) setPage(pc);
  }, [entries.data, page, pageSize]);

  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkNotice, setBulkNotice] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const runBulk = async (verb: "publish" | "unpublish" | "delete") => {
    const ids = [...selected];
    setBulkBusy(true);
    setBulkNotice(null);
    let failed = 0;
    for (const id of ids) {
      try {
        if (verb === "publish") await publishEntry(type, id);
        else if (verb === "unpublish") await unpublishEntry(type, id);
        else await deleteEntry(type, id);
      } catch {
        failed += 1;
      }
    }
    setBulkBusy(false);
    setConfirmingDelete(false);
    setSelected([]);
    if (failed > 0) setBulkNotice(`${verb} failed for ${failed} of ${ids.length} entries.`);
    entries.refetch();
  };
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

  const rows = entries.data?.data ?? [];

  // Filter refetches flip entries.loading; keep the table (and the open
  // filters popover) mounted — full-page loader only before first data.
  if (schema.loading || (entries.loading && !entries.data)) return <LoadingState />;
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
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  // Sliding window of up to 5 page buttons, current page centered.
  const winStart = Math.max(1, Math.min(page - 2, pageCount - 4));
  const pageWindow = Array.from(
    { length: Math.min(5, pageCount) },
    (_, i) => winStart + i,
  );

  const targetSchema = (f: Field): ContentType | undefined => {
    const m = relationMeta(f);
    return m ? allTypes.data?.find((t) => t.name === m.target) : undefined;
  };

  const allOn = rows.length > 0 && selected.length === rows.length;
  const toggleAll = () => setSelected(allOn ? [] : rows.map((e) => e.id));
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
              {l} {statusFilter === k && <span className="rs-tab-count">{total}</span>}
            </button>
          ))}
        </div>
      )}

      <div className="rs-cm-toolbar">
        {titleField && (
          <div className="rs-search rs-search--inline">
            <Icons.search size={15} />
            <input
              placeholder={`Search ${ct.display_name.toLowerCase()}`}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
        )}
        <div className="rs-pop-anchor">
          <button
            className={"rs-btn rs-btn--ghost" + (activeFilterCount > 0 ? " is-active" : "")}
            onClick={() => {
              // Seed an empty rule on open so the first condition is one
              // click closer — no need to hit "Add filter" first.
              if (!filtersOpen && filterRules.length === 0) {
                const fs = filterableFields(ct);
                if (fs.length > 0) setFilterRules([makeRule(fs[0])]);
              }
              setFiltersOpen(!filtersOpen);
            }}
          >
            <Icons.filter size={15} /> Filters{activeFilterCount > 0 ? ` · ${activeFilterCount}` : ""}
          </button>
          {filtersOpen && (
            <FiltersMenu
              ct={ct}
              allTypes={allTypes.data}
              rules={filterRules}
              setRules={setFilterRules}
              onClose={() => setFiltersOpen(false)}
            />
          )}
        </div>
        {titleField && (
          <button
            className="rs-btn rs-btn--ghost"
            onClick={() => setSortKey(sortKey === "title" ? "updated" : "title")}
          >
            <Icons.sort size={15} /> {sortKey === "title" ? "Title" : "Last update"}
          </button>
        )}
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

      {bulkNotice && (
        <div className="rs-notice rs-notice--warn">
          <span>{bulkNotice}</span>
          <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setBulkNotice(null)}>
            Dismiss
          </button>
        </div>
      )}

      {selected.length > 0 && (
        <div className="rs-bulkbar">
          <span><strong>{selected.length}</strong> selected</span>
          <div className="rs-bulkbar-actions">
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={handleExport} disabled={bulkBusy}>
              <Icons.doc size={14} /> Export CSV
            </button>
            {dp && (
              <>
                <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => void runBulk("publish")} disabled={bulkBusy}>
                  <Icons.eye size={14} /> Publish
                </button>
                <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => void runBulk("unpublish")} disabled={bulkBusy}>
                  <Icons.x size={14} /> Unpublish
                </button>
              </>
            )}
            <button className="rs-btn rs-btn--ghost rs-btn--sm rs-danger" onClick={() => setConfirmingDelete(true)} disabled={bulkBusy}>
              <Icons.trash size={14} /> Delete
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setSelected([])} disabled={bulkBusy}>Clear</button>
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
            {rows.map((e) => (
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
        {rows.length === 0 && <div className="rs-empty">No entries match.</div>}
      </div>

      <div className="rs-pager">
        <span className="rs-cell-muted">Showing {rows.length} of {total}</span>
        <div className="rs-pager-ctrl">
          <button className="rs-page-btn" disabled={page <= 1} onClick={() => setPage(page - 1)}>
            <Icons.chevLeft size={16} />
          </button>
          {pageWindow.map((p) => (
            <button
              key={p}
              className={"rs-page-btn" + (p === page ? " is-active" : "")}
              onClick={() => setPage(p)}
            >
              {p}
            </button>
          ))}
          <button className="rs-page-btn" disabled={page >= pageCount} onClick={() => setPage(page + 1)}>
            <Icons.chevRight size={16} />
          </button>
          <select
            className="rs-select-sm"
            value={pageSize}
            onChange={(e) => setPageSize(Number(e.target.value))}
          >
            <option value={25}>25 / page</option>
            <option value={50}>50 / page</option>
            <option value={100}>100 / page</option>
          </select>
        </div>
      </div>

      {confirmingDelete && (
        <div className="rs-modal-backdrop" onClick={() => { if (!bulkBusy) setConfirmingDelete(false); }}>
          <div
            className="rs-modal"
            role="dialog"
            aria-modal="true"
            onClick={(e) => e.stopPropagation()}
            style={{ maxWidth: 420 }}
          >
            <div className="rs-modal-head">
              <div className="rs-modal-ico" style={{ background: "var(--danger-soft, var(--surface-3))", color: "var(--danger)" }}>
                <Icons.trash size={18} />
              </div>
              <div className="rs-modal-titles">
                <span className="rs-modal-eyebrow">Destructive action</span>
                <h2>Delete {selected.length} {selected.length === 1 ? "entry" : "entries"}?</h2>
              </div>
              <button className="rs-modal-x" onClick={() => setConfirmingDelete(false)} disabled={bulkBusy} aria-label="Close">
                <Icons.x size={18} />
              </button>
            </div>
            <div className="rs-modal-body">
              <p style={{ fontSize: 14, color: "var(--text-muted)", margin: 0 }}>
                This permanently deletes the selected entries. This cannot be undone.
              </p>
            </div>
            <div className="rs-modal-foot" style={{ justifyContent: "space-between" }}>
              <button className="rs-btn rs-btn--ghost" onClick={() => setConfirmingDelete(false)} disabled={bulkBusy}>
                Cancel
              </button>
              <button
                className="rs-btn rs-btn--primary"
                onClick={() => void runBulk("delete")}
                disabled={bulkBusy}
                style={{ background: "var(--danger)", borderColor: "var(--danger)", color: "#fff" }}
              >
                {bulkBusy ? "Deleting…" : "Delete entries"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

