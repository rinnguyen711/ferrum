import { Icons } from "../components/icons";

const ASSETS = [
  { id: 1, name: "estuary-dawn.jpg", dim: "4096×2731", size: "3.2 MB", hue: 195, ext: "JPG" },
  { id: 2, name: "buried-river-map.png", dim: "2400×3000", size: "1.8 MB", hue: 28, ext: "PNG" },
  { id: 3, name: "coral-tank-01.jpg", dim: "3000×2000", size: "2.6 MB", hue: 270, ext: "JPG" },
  { id: 4, name: "night-train-window.jpg", dim: "3600×2400", size: "4.1 MB", hue: 220, ext: "JPG" },
  { id: 5, name: "lichen-macro.jpg", dim: "2800×2800", size: "2.0 MB", hue: 95, ext: "JPG" },
  { id: 6, name: "sauna-steam.jpg", dim: "3200×2133", size: "2.9 MB", hue: 18, ext: "JPG" },
];

export function MediaLibrary() {
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Media Library <span className="rs-preview-pill">preview</span></h1>
          <p className="rs-cm-sub">{ASSETS.length} sample assets · uploads not yet wired</p>
        </div>
        <button className="rs-btn rs-btn--primary" data-placeholder title="Coming soon">
          <Icons.plus size={16} /> Upload assets
        </button>
      </div>
      <div className="rs-media-grid">
        {ASSETS.map((m) => (
          <div className="rs-media-card" key={m.id}>
            <div
              className="rs-media-cover"
              style={{ background: `linear-gradient(135deg, hsl(${m.hue} 50% 80%), hsl(${m.hue + 18} 45% 62%))` }}
            >
              <span className="rs-media-ext rs-mono">{m.ext}</span>
            </div>
            <div className="rs-media-card-meta">
              <strong title={m.name}>{m.name}</strong>
              <span className="rs-cell-muted rs-mono">{m.dim} · {m.size}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
