import { Icons } from "../components/icons";
import { RUSTAPI } from "../mock/data";

export function MediaLibrary() {
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Media Library</h1>
          <p className="rs-cm-sub">{RUSTAPI.media.length} assets · 22.7 MB</p>
        </div>
        <button className="rs-btn rs-btn--primary">
          <Icons.plus size={16} /> Upload assets
        </button>
      </div>
      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input placeholder="Search assets" />
        </div>
        <button className="rs-btn rs-btn--ghost">
          <Icons.filter size={15} /> Type
        </button>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--ghost">
          <Icons.sort size={15} /> Newest
        </button>
      </div>
      <div className="rs-media-grid">
        {RUSTAPI.media.map((m) => (
          <div className="rs-media-card" key={m.id}>
            <div
              className="rs-media-cover"
              style={{
                background: `linear-gradient(135deg, hsl(${m.hue} 50% 80%), hsl(${m.hue + 18} 45% 62%))`,
              }}
            >
              <span className="rs-media-ext rs-mono">{m.ext}</span>
            </div>
            <div className="rs-media-card-meta">
              <strong title={m.name}>{m.name}</strong>
              <span className="rs-cell-muted rs-mono">
                {m.w}×{m.h} · {m.size}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
