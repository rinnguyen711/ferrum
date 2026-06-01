import { useState, type CSSProperties, type ReactNode } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Avatar, STATUS, StatusBadge } from "../components/shell";
import { Icons } from "../components/icons";
import {
  RUSTAPI,
  relTime,
  type Article,
  type Author,
  type Status,
} from "../mock/data";

type EntryId = number | "new";
type Tab = "content" | "seo" | "api";

interface Form {
  title: string;
  slug: string;
  status: Status;
  excerpt: string;
  author: number;
  categories: number[];
  featured: boolean;
  readTime: number;
}

export function EntryEditor() {
  const { type = "article", id = "new" } = useParams<{ type: string; id: string }>();
  const navigate = useNavigate();
  const entryId: EntryId = id === "new" ? "new" : Number(id);
  const onBack = () => navigate(`/content/${type}`);

  const isNew = entryId === "new";
  const base: Article | null = isNew
    ? null
    : RUSTAPI.articles.find((a) => a.id === entryId) ?? null;

  const [form, setForm] = useState<Form>(() =>
    base
      ? {
          title: base.title,
          slug: base.slug,
          status: base.status,
          excerpt: base.excerpt,
          author: base.author,
          categories: [...base.categories],
          featured: base.featured,
          readTime: base.readTime,
        }
      : {
          title: "",
          slug: "",
          status: "draft",
          excerpt: "",
          author: 1,
          categories: [],
          featured: false,
          readTime: 5,
        },
  );
  const [dirty, setDirty] = useState(false);
  const [tab, setTab] = useState<Tab>("content");

  const set = <K extends keyof Form>(k: K, v: Form[K]) => {
    setForm((f) => ({ ...f, [k]: v }));
    setDirty(true);
  };
  const au = RUSTAPI.authors.find((x) => x.id === form.author)!;

  return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={onBack}>
          <Icons.arrowLeft size={18} />
        </button>
        <div className="rs-editor-titlewrap">
          <h1>{isNew ? "Create an entry" : form.title || "Untitled"}</h1>
          <div className="rs-editor-meta">
            <StatusBadge status={form.status} />
            {!isNew && base && (
              <span className="rs-cell-muted">
                · API ID <code className="rs-mono">article::{base.id}</code>
              </span>
            )}
            {dirty && (
              <span className="rs-unsaved">
                <span className="rs-dot" /> Unsaved changes
              </span>
            )}
          </div>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost">
            <Icons.eye size={15} /> Preview
          </button>
          <button
            className="rs-btn rs-btn--ghost"
            disabled={!dirty}
            onClick={() => setDirty(false)}
          >
            Save draft
          </button>
          <button className="rs-btn rs-btn--primary">
            <Icons.bolt size={15} />{" "}
            {form.status === "published" ? "Update & publish" : "Publish"}
          </button>
        </div>
      </div>

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-editor-tabs">
            {([
              ["content", "Content"],
              ["seo", "SEO & meta"],
              ["api", "API & preview"],
            ] as [Tab, string][]).map(([k, l]) => (
              <button
                key={k}
                className={"rs-etab" + (tab === k ? " is-active" : "")}
                onClick={() => setTab(k)}
              >
                {l}
              </button>
            ))}
          </div>

          {tab === "content" && (
            <div className="rs-fields">
              <Field label="Title" required>
                <input
                  className="rs-input rs-input--lg"
                  value={form.title}
                  placeholder="A clear, declarative headline"
                  onChange={(e) => set("title", e.target.value)}
                />
              </Field>
              <Field label="Slug" required hint="UID · auto-generated from title">
                <div className="rs-input-affix">
                  <span className="rs-affix rs-mono">/journal/</span>
                  <input
                    className="rs-input rs-mono"
                    value={form.slug}
                    onChange={(e) => set("slug", e.target.value)}
                  />
                  <button className="rs-affix-btn" title="Regenerate">
                    <Icons.bolt size={14} />
                  </button>
                </div>
              </Field>
              <Field label="Cover image" hint="Media · single">
                <MediaSlot />
              </Field>
              <Field label="Excerpt" hint="Long text · shown in listings">
                <textarea
                  className="rs-input rs-textarea"
                  rows={2}
                  value={form.excerpt}
                  onChange={(e) => set("excerpt", e.target.value)}
                  placeholder="One or two sentences."
                />
              </Field>
              <Field label="Body" required hint="Rich text · blocks">
                <RichTextField />
              </Field>
            </div>
          )}

          {tab === "seo" && (
            <div className="rs-fields">
              <Field label="Meta title" hint="Recommended 50–60 characters">
                <input className="rs-input" defaultValue={form.title} />
              </Field>
              <Field label="Meta description" hint="Recommended 140–160 characters">
                <textarea className="rs-input rs-textarea" rows={3} defaultValue={form.excerpt} />
              </Field>
              <Field label="Canonical URL">
                <input
                  className="rs-input rs-mono"
                  defaultValue={"https://aurora.journal/journal/" + form.slug}
                />
              </Field>
              <Field label="Open Graph image" hint="Falls back to cover image">
                <MediaSlot />
              </Field>
            </div>
          )}

          {tab === "api" && <ApiPreview form={form} base={base} au={au} />}
        </div>

        <aside className="rs-editor-rail">
          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.bolt size={15} /> Publish
            </div>
            <div className="rs-rail-field">
              <label>Status</label>
              <div className="rs-segment">
                {(["draft", "review", "published"] as Status[]).map((s) => (
                  <button
                    key={s}
                    className={"rs-seg" + (form.status === s ? " is-active" : "")}
                    onClick={() => set("status", s)}
                  >
                    {STATUS[s].label}
                  </button>
                ))}
              </div>
            </div>
            <div className="rs-rail-field">
              <label>Featured</label>
              <Toggle on={form.featured} onChange={(v) => set("featured", v)} />
            </div>
            <div className="rs-rail-divider" />
            <div className="rs-rail-stat">
              <span>Created</span>
              <strong>{base ? "May 17, 2026" : "Just now"}</strong>
            </div>
            <div className="rs-rail-stat">
              <span>Last update</span>
              <strong>{base ? relTime(base.updatedAt) : "—"}</strong>
            </div>
            <div className="rs-rail-stat">
              <span>Published</span>
              <strong>{base && base.publishedAt ? "May 28, 2026" : "Not yet"}</strong>
            </div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.relation size={15} /> Author
            </div>
            <select
              className="rs-input"
              value={form.author}
              onChange={(e) => set("author", +e.target.value)}
            >
              {RUSTAPI.authors.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
            <div className="rs-rail-author">
              <Avatar name={au.name} initials={au.avatar} color={au.color} size={32} />
              <div>
                <strong>{au.name}</strong>
                <span className="rs-cell-muted">{au.role}</span>
              </div>
            </div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.tag size={15} /> Categories
            </div>
            <div className="rs-chips rs-chips--wrap">
              {RUSTAPI.categories.map((c) => {
                const on = form.categories.includes(c.id);
                return (
                  <button
                    key={c.id}
                    className={"rs-chip rs-chip--toggle" + (on ? " is-on" : "")}
                    style={{ ["--chip" as string]: c.color } as CSSProperties}
                    onClick={() =>
                      set(
                        "categories",
                        on
                          ? form.categories.filter((x) => x !== c.id)
                          : [...form.categories, c.id],
                      )
                    }
                  >
                    {on && <Icons.check size={12} />} {c.name}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="rs-rail-card">
            <div className="rs-rail-card-head">
              <Icons.clock size={15} /> Read time
            </div>
            <div className="rs-stepper">
              <button onClick={() => set("readTime", Math.max(1, form.readTime - 1))}>−</button>
              <span className="rs-mono">{form.readTime} min</span>
              <button onClick={() => set("readTime", form.readTime + 1)}>+</button>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
}

function Field({
  label,
  required,
  hint,
  children,
}: {
  label: string;
  required?: boolean;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="rs-field">
      <div className="rs-field-label">
        <label>
          {label}
          {required && <span className="rs-req">*</span>}
        </label>
        {hint && <span className="rs-field-hint">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      className={"rs-toggle" + (on ? " is-on" : "")}
      onClick={() => onChange(!on)}
    >
      <span className="rs-toggle-knob" />
    </button>
  );
}

function MediaSlot() {
  return (
    <div className="rs-media-slot">
      <div
        className="rs-media-thumb"
        style={{ background: "linear-gradient(135deg, hsl(200 45% 82%), hsl(200 40% 68%))" }}
      >
        <Icons.image size={22} />
      </div>
      <div className="rs-media-info">
        <strong>estuary-dawn.jpg</strong>
        <span className="rs-cell-muted rs-mono">4096 × 2731 · 3.2 MB</span>
        <div className="rs-media-actions">
          <button className="rs-link-btn">Replace</button>
          <button className="rs-link-btn rs-danger">Remove</button>
        </div>
      </div>
    </div>
  );
}

function RichTextField() {
  const buttons: Array<{ label: string; style?: CSSProperties }> = [
    { label: "B", style: { fontWeight: 700 } },
    { label: "i", style: { fontStyle: "italic" } },
    { label: "“" },
    { label: "H1" },
    { label: "H2" },
  ];
  return (
    <div className="rs-rte">
      <div className="rs-rte-toolbar">
        {buttons.map((b, i) => (
          <button key={i} className="rs-rte-btn" style={b.style}>
            {b.label}
          </button>
        ))}
        <span className="rs-rte-sep" />
        <button className="rs-rte-btn">
          <Icons.image size={15} />
        </button>
        <button className="rs-rte-btn">
          <Icons.link size={15} />
        </button>
        <button className="rs-rte-btn">
          <Icons.hash size={15} />
        </button>
        <div className="rs-spacer" />
        <button className="rs-rte-btn rs-mono" style={{ fontSize: 12 }}>
          Markdown
        </button>
      </div>
      <div className="rs-rte-body">
        <p>
          The estuary at Strangford has run a turbine since 2008, but the new{" "}
          <strong>low-speed rotors</strong> change the maths entirely. Where the old machines
          needed a two-metre tidal race, these spin in water that barely seems to move.
        </p>
        <p>
          “We stopped designing for the peak and started designing for the average,” says the lead
          engineer, standing on a pontoon at slack water. It is a small sentence with a large
          consequence.
        </p>
        <p>Across the channel, a second array is already feeding the grid——</p>
        <span className="rs-rte-caret" />
      </div>
    </div>
  );
}

function ApiPreview({
  form,
  base,
  au,
}: {
  form: Form;
  base: Article | null;
  au: Author;
}) {
  const cats = form.categories
    .map((id) => RUSTAPI.categories.find((c) => c.id === id))
    .filter((c): c is NonNullable<typeof c> => Boolean(c));
  const json = {
    data: {
      id: base ? base.id : "—",
      type: "article",
      attributes: {
        title: form.title,
        slug: form.slug,
        status: form.status,
        excerpt: form.excerpt,
        readTime: form.readTime,
        featured: form.featured,
        author: { data: { id: au.id, name: au.name } },
        categories: { data: cats.map((c) => ({ id: c.id, name: c.name })) },
        publishedAt: base && base.publishedAt ? base.publishedAt : null,
      },
    },
    meta: { locale: "en" },
  };
  return (
    <div className="rs-api">
      <div className="rs-api-row">
        <span className="rs-method">GET</span>
        <code className="rs-mono rs-api-url">
          {"/api/articles/" + (base ? base.id : ":id")}
        </code>
        <button className="rs-btn rs-btn--ghost rs-btn--sm">
          <Icons.copy size={14} /> Copy
        </button>
        <button className="rs-btn rs-btn--ghost rs-btn--sm">
          <Icons.external size={14} /> Open
        </button>
      </div>
      <div className="rs-api-note">
        <Icons.bolt size={14} />
        Served by the Rust API (axum + sqlx). Typed end-to-end — this response is generated from
        the same schema as the form above.
      </div>
      <pre className="rs-code">
        <code>{JSON.stringify(json, null, 2)}</code>
      </pre>
      <div className="rs-api-meta">
        <div>
          <span>Response</span>
          <strong className="rs-mono">200 OK</strong>
        </div>
        <div>
          <span>Latency</span>
          <strong className="rs-mono">8.4 ms</strong>
        </div>
        <div>
          <span>Size</span>
          <strong className="rs-mono">1.2 KB</strong>
        </div>
        <div>
          <span>Cache</span>
          <strong className="rs-mono">HIT</strong>
        </div>
      </div>
    </div>
  );
}
