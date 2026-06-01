import { useState, type CSSProperties } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Avatar, StatusBadge } from "../components/shell";
import { Icons } from "../components/icons";
import {
  RUSTAPI,
  relTime,
  type Article,
  type ContentType,
  type Status,
} from "../mock/data";

type EntryId = number | "new";

export function Checkbox({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <button
      className={"rs-check" + (checked ? " is-on" : "")}
      onClick={onChange}
      role="checkbox"
      aria-checked={checked}
    >
      {checked && <Icons.check size={13} />}
    </button>
  );
}

export function ContentList() {
  const { type = "article" } = useParams<{ type: string }>();
  const navigate = useNavigate();
  const t = RUSTAPI.types[type];
  if (!t) return <div className="rs-empty">Unknown content type: {type}</div>;
  const onOpen = (id: EntryId) => navigate(`/content/${type}/${id}`);
  if (type === "article") return <ArticleList t={t} onOpen={onOpen} />;
  if (type === "author") return <AuthorList t={t} onOpen={onOpen} />;
  return <CategoryList t={t} onOpen={onOpen} />;
}

type StatusFilter = "all" | Status;
type SortKey = "title" | "updatedAt";

function ArticleList({
  t: _t,
  onOpen,
}: {
  t: ContentType;
  onOpen: (id: EntryId) => void;
}) {
  const [selected, setSelected] = useState<number[]>([]);
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [sortKey, setSortKey] = useState<SortKey>("updatedAt");

  let rows: Article[] = RUSTAPI.articles.filter((a) => {
    if (statusFilter !== "all" && a.status !== statusFilter) return false;
    if (query && !a.title.toLowerCase().includes(query.toLowerCase())) return false;
    return true;
  });
  rows = [...rows].sort((a, b) => {
    if (sortKey === "title") return a.title.localeCompare(b.title);
    return new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime();
  });

  const authorOf = (id: number) => RUSTAPI.authors.find((x) => x.id === id)!;
  const catOf = (id: number) => RUSTAPI.categories.find((x) => x.id === id)!;
  const allOn = rows.length > 0 && selected.length === rows.length;
  const toggleAll = () => setSelected(allOn ? [] : rows.map((r) => r.id));
  const toggle = (id: number) =>
    setSelected(selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id]);

  const counts = {
    all: RUSTAPI.articles.length,
    published: RUSTAPI.articles.filter((a) => a.status === "published").length,
    draft: RUSTAPI.articles.filter((a) => a.status === "draft").length,
    review: RUSTAPI.articles.filter((a) => a.status === "review").length,
  };

  const tabs: [StatusFilter, string][] = [
    ["all", "All"],
    ["published", "Published"],
    ["review", "In review"],
    ["draft", "Draft"],
  ];

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Articles</h1>
          <p className="rs-cm-sub">
            {counts.all} entries · last edited {relTime(RUSTAPI.articles[0].updatedAt)}
          </p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => onOpen("new")}>
          <Icons.plus size={16} /> Create new entry
        </button>
      </div>

      <div className="rs-cm-tabs">
        {tabs.map(([k, l]) => (
          <button
            key={k}
            className={"rs-tab" + (statusFilter === k ? " is-active" : "")}
            onClick={() => setStatusFilter(k)}
          >
            {l} <span className="rs-tab-count">{counts[k]}</span>
          </button>
        ))}
      </div>

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input
            placeholder="Search articles"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button className="rs-btn rs-btn--ghost">
          <Icons.filter size={15} /> Filters
        </button>
        <button
          className="rs-btn rs-btn--ghost"
          onClick={() => setSortKey(sortKey === "title" ? "updatedAt" : "title")}
        >
          <Icons.sort size={15} /> {sortKey === "title" ? "Title" : "Last update"}
        </button>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--ghost">
          <Icons.layers size={15} /> Fields
        </button>
      </div>

      {selected.length > 0 && (
        <div className="rs-bulkbar">
          <span>
            <strong>{selected.length}</strong> selected
          </span>
          <div className="rs-bulkbar-actions">
            <button className="rs-btn rs-btn--ghost rs-btn--sm">
              <Icons.eye size={14} /> Publish
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm">
              <Icons.x size={14} /> Unpublish
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm rs-danger">
              <Icons.trash size={14} /> Delete
            </button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setSelected([])}>
              Clear
            </button>
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
              <th className="rs-col-id">ID</th>
              <th>Title</th>
              <th>Status</th>
              <th>Author</th>
              <th>Categories</th>
              <th>Updated</th>
              <th className="rs-col-act" />
            </tr>
          </thead>
          <tbody>
            {rows.map((a) => {
              const au = authorOf(a.author);
              return (
                <tr
                  key={a.id}
                  className={selected.includes(a.id) ? "is-selected" : ""}
                  onClick={() => onOpen(a.id)}
                >
                  <td className="rs-col-check" onClick={(e) => e.stopPropagation()}>
                    <Checkbox checked={selected.includes(a.id)} onChange={() => toggle(a.id)} />
                  </td>
                  <td className="rs-col-id rs-mono">{a.id}</td>
                  <td className="rs-cell-title">
                    {a.featured && <Icons.star size={13} className="rs-feat" />}
                    <span className="rs-title-text">{a.title}</span>
                  </td>
                  <td>
                    <StatusBadge status={a.status} />
                  </td>
                  <td>
                    <span className="rs-cell-author">
                      <Avatar name={au.name} initials={au.avatar} color={au.color} size={22} />
                      {au.name}
                    </span>
                  </td>
                  <td>
                    <span className="rs-chips">
                      {a.categories.map((cid) => {
                        const c = catOf(cid);
                        return (
                          <span
                            key={cid}
                            className="rs-chip"
                            style={{ ["--chip" as string]: c.color } as CSSProperties}
                          >
                            {c.name}
                          </span>
                        );
                      })}
                    </span>
                  </td>
                  <td className="rs-cell-muted">{relTime(a.updatedAt)}</td>
                  <td className="rs-col-act" onClick={(e) => e.stopPropagation()}>
                    <button className="rs-row-btn" onClick={() => onOpen(a.id)}>
                      <Icons.edit size={16} />
                    </button>
                    <button className="rs-row-btn">
                      <Icons.dots size={16} />
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {rows.length === 0 && <div className="rs-empty">No entries match your filters.</div>}
      </div>

      <div className="rs-pager">
        <span className="rs-cell-muted">
          Showing {rows.length} of {counts.all}
        </span>
        <div className="rs-pager-ctrl">
          <button className="rs-page-btn" disabled>
            <Icons.chevLeft size={16} />
          </button>
          <button className="rs-page-btn is-active">1</button>
          <button className="rs-page-btn">2</button>
          <button className="rs-page-btn">
            <Icons.chevRight size={16} />
          </button>
          <select className="rs-select-sm">
            <option>25 / page</option>
            <option>50 / page</option>
          </select>
        </div>
      </div>
    </div>
  );
}

function AuthorList({
  t: _t,
  onOpen,
}: {
  t: ContentType;
  onOpen: (id: EntryId) => void;
}) {
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Authors</h1>
          <p className="rs-cm-sub">{RUSTAPI.authors.length} entries</p>
        </div>
        <button className="rs-btn rs-btn--primary">
          <Icons.plus size={16} /> Create new entry
        </button>
      </div>
      <div className="rs-cards">
        {RUSTAPI.authors.map((a) => (
          <div className="rs-author-card" key={a.id} onClick={() => onOpen(a.id)}>
            <Avatar name={a.name} initials={a.avatar} color={a.color} size={48} />
            <div className="rs-author-meta">
              <strong>{a.name}</strong>
              <span className="rs-cell-muted">{a.role}</span>
            </div>
            <p className="rs-author-bio">{a.bio}</p>
          </div>
        ))}
      </div>
    </div>
  );
}

function CategoryList({
  t: _t,
  onOpen,
}: {
  t: ContentType;
  onOpen: (id: EntryId) => void;
}) {
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Categories</h1>
          <p className="rs-cm-sub">{RUSTAPI.categories.length} entries</p>
        </div>
        <button className="rs-btn rs-btn--primary">
          <Icons.plus size={16} /> Create new entry
        </button>
      </div>
      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th className="rs-col-id">ID</th>
              <th>Name</th>
              <th>Slug</th>
              <th>Color</th>
              <th>Entries</th>
            </tr>
          </thead>
          <tbody>
            {RUSTAPI.categories.map((c) => (
              <tr key={c.id} onClick={() => onOpen(c.id)}>
                <td className="rs-col-id rs-mono">{c.id}</td>
                <td className="rs-cell-title">
                  <span className="rs-title-text">{c.name}</span>
                </td>
                <td className="rs-mono rs-cell-muted">{c.slug}</td>
                <td>
                  <span className="rs-swatch" style={{ background: c.color }} />
                  <span className="rs-mono rs-cell-muted">{c.color}</span>
                </td>
                <td className="rs-cell-muted">{c.count}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
