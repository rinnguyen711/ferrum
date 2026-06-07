import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar, StatusBadge } from "../components/shell";
import { FieldsMenu, type ColumnDef } from "./FieldsMenu";
import { useResource } from "../hooks/useResource";
import { getContentType, listContentTypes, listEntries } from "../api/endpoints";
import type { ContentType, Entry, Field } from "../api/types";
import { draftPublishEnabled, relationMeta } from "../api/types";
import { relTime, relationLabel, shortId } from "../util";

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

  const schema = useResource(() => getContentType(type), [type]);
  const allTypes = useResource(() => listContentTypes(), []);

  const ct = schema.data;
  const dp = ct ? draftPublishEnabled(ct) : false;
  const populate = ct
    ? ct.fields.filter((f) => f.kind === "relation").map((f) => f.name).join(",")
    : "";

  const [publishFilter, setPublishFilter] = useState<"published" | "draft" | "all">("published");

  const entries = useResource(
    () => listEntries(type, { populate: populate || undefined, pageSize: 100, status: dp ? publishFilter : undefined }),
    [type, populate, dp, publishFilter],
  );

  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [selected, setSelected] = useState<string[]>([]);
  const [fieldsOpen, setFieldsOpen] = useState(false);

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

  if (schema.loading || entries.loading) return <div className="rs-empty">Loading…</div>;
  if (schema.error)
    return (
      <div className="rs-empty">
        Couldn’t load type “{type}”.{" "}
        <button className="rs-link-btn" onClick={schema.refetch}>Retry</button>
      </div>
    );
  if (entries.error)
    return (
      <div className="rs-empty">
        {entries.error.message}{" "}
        <button className="rs-link-btn" onClick={entries.refetch}>Retry</button>
      </div>
    );
  if (!ct || !entries.data) return <div className="rs-empty">Unknown content type.</div>;

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
              <Avatar name={String(label)} initials={initials(String(label))} color="#52525B" size={22} />
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
      </div>

      {selected.length > 0 && (
        <div className="rs-bulkbar">
          <span><strong>{selected.length}</strong> selected</span>
          <div className="rs-bulkbar-actions">
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
                    {e.published_at
                      ? <span className="rs-status rs-status--ok">Published</span>
                      : <span className="rs-status rs-status--muted">Draft</span>}
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

function Checkbox({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      className={"rs-check" + (checked ? " is-on" : "")}
      onClick={onChange}
      role="checkbox"
      aria-checked={checked}
      type="button"
    >
      {checked && <Icons.check size={13} />}
    </button>
  );
}

function initials(s: string): string {
  return s.split(/\s+/).map((w) => w[0] ?? "").join("").slice(0, 2).toUpperCase() || "?";
}
