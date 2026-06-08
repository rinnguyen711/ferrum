import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Icons } from "../../components/icons";
import { listFolders, listAssets, uploadAsset } from "../../api/endpoints";
import type { MediaFolder, MediaAsset } from "../../api/types";
import { AssetThumb } from "./AssetThumb";
import { SelectBox } from "./SelectBox";

interface Staged { file: File; sid: string; }
let _seq = 0;

export function AssetPicker({
  multiple,
  onClose,
  onPick,
}: {
  multiple: boolean;
  onClose: () => void;
  onPick: (assets: MediaAsset[]) => void;
}) {
  const [tab, setTab] = useState<"browse" | "upload">("browse");
  const [folders, setFolders] = useState<MediaFolder[]>([]);
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [cur, setCur] = useState<string | null>(null);
  const [picked, setPicked] = useState<MediaAsset[]>([]);

  useEffect(() => { listFolders({ all: true }).then(setFolders).catch(() => {}); }, []);
  const reload = useCallback((folderId: string | null) => {
    listAssets(folderId).then(setAssets).catch(() => {});
  }, []);
  useEffect(() => { reload(cur); }, [cur, reload]);

  const path = useMemo(() => {
    const chain: MediaFolder[] = [];
    let id: string | null = cur;
    const byId = new Map(folders.map((f) => [f.id, f]));
    while (id != null) {
      const f = byId.get(id);
      if (!f) break;
      chain.unshift(f);
      id = f.parent_id;
    }
    return chain;
  }, [cur, folders]);

  const subFolders = folders.filter((f) => f.parent_id === cur);
  const pickedIds = new Set(picked.map((a) => a.id));
  const toggle = (a: MediaAsset) => {
    if (!multiple) { onPick([a]); return; }
    setPicked((p) => (pickedIds.has(a.id) ? p.filter((x) => x.id !== a.id) : [...p, a]));
  };

  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div className="rs-modal rs-modal--wide" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <div className="rs-modal-head">
          <div className="rs-modal-icon"><Icons.image size={18} /></div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">Media Library</span>
            <h2>{multiple ? "Select assets" : "Select an asset"}</h2>
          </div>
          <div className="rs-modal-tabs" style={{ margin: "0 0 0 auto", border: "none", alignSelf: "stretch", display: "flex", alignItems: "flex-end" }}>
            <button className={"rs-modal-tab" + (tab === "browse" ? " is-on" : "")} onClick={() => setTab("browse")} type="button">Browse</button>
            <button className={"rs-modal-tab" + (tab === "upload" ? " is-on" : "")} onClick={() => setTab("upload")} type="button">Upload new</button>
          </div>
          <button className="rs-modal-x" onClick={onClose}><Icons.x size={18} /></button>
        </div>

        {tab === "browse" ? (
          <>
            <div className="rs-media-bc rs-picker-bc">
              {path.length === 0
                ? <span className="rs-media-bc-here">All</span>
                : <button onClick={() => setCur(null)} type="button">All</button>}
              {path.map((f, i) => (
                <span key={f.id} style={{ display: "contents" }}>
                  <span className="rs-media-bc-sep">/</span>
                  {i === path.length - 1
                    ? <span className="rs-media-bc-here">{f.name}</span>
                    : <button onClick={() => setCur(f.id)} type="button">{f.name}</button>}
                </span>
              ))}
            </div>

            <div className="rs-modal-body rs-picker-body">
              {subFolders.length > 0 && (
                <div className="rs-picker-folders">
                  {subFolders.map((f) => (
                    <button key={f.id} className="rs-picker-folder" onClick={() => setCur(f.id)} type="button">
                      <Icons.folder size={13} />
                      <span title={f.name}>{f.name}</span>
                    </button>
                  ))}
                </div>
              )}
              {assets.length === 0 ? (
                <div className="rs-media-empty"><p>No assets in this folder.</p></div>
              ) : (
                <div className="rs-picker-grid">
                  {assets.map((m) => {
                    const sel = pickedIds.has(m.id);
                    return (
                      <div className={"rs-picker-card" + (sel ? " is-selected" : "")} key={m.id} onClick={() => toggle(m)}>
                        <div className="rs-picker-check" onClick={(e) => { e.stopPropagation(); toggle(m); }}>
                          <SelectBox checked={sel} />
                        </div>
                        <AssetThumb asset={m} className="rs-picker-thumb" />
                        <div className="rs-picker-name" title={m.file_name}>{m.file_name}</div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {multiple && (
              <div className="rs-modal-foot">
                <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
                <div className="rs-spacer" />
                <button className="rs-btn rs-btn--primary" disabled={picked.length === 0} onClick={() => onPick(picked)}>
                  <Icons.check size={15} /> Add {picked.length || ""} asset{picked.length === 1 ? "" : "s"}
                </button>
              </div>
            )}
          </>
        ) : (
          <UploadTab folderId={cur} folders={folders} onUploaded={(assets) => { reload(cur); setTab("browse"); onPick(assets); }} onClose={onClose} />
        )}
      </div>
    </div>
  );
}

function UploadTab({
  folderId,
  folders,
  onUploaded,
  onClose,
}: {
  folderId: string | null;
  folders: MediaFolder[];
  onUploaded: (assets: MediaAsset[]) => void;
  onClose: () => void;
}) {
  const [staged, setStaged] = useState<Staged[]>([]);
  const [dest, setDest] = useState<string | null>(folderId);
  const [over, setOver] = useState(false);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const addFiles = (files: FileList | File[]) => {
    const next = Array.from(files).map((file) => ({ file, sid: "s" + (++_seq) }));
    setStaged((s) => [...s, ...next]);
  };

  const upload = async () => {
    if (!staged.length || busy) return;
    setBusy(true);
    try {
      const results = await Promise.allSettled(staged.map((s) => uploadAsset(s.file, dest)));
      const uploaded = results.filter((r): r is PromiseFulfilledResult<MediaAsset> => r.status === "fulfilled").map((r) => r.value);
      onUploaded(uploaded);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="rs-modal-body rs-picker-body">
        <input
          ref={inputRef}
          type="file"
          multiple
          style={{ display: "none" }}
          onChange={(e) => { if (e.target.files) addFiles(e.target.files); e.target.value = ""; }}
        />
        <div
          className={"rs-dropzone" + (over ? " is-over" : "")}
          onClick={() => inputRef.current?.click()}
          onDragOver={(e) => { e.preventDefault(); setOver(true); }}
          onDragLeave={() => setOver(false)}
          onDrop={(e) => { e.preventDefault(); setOver(false); if (e.dataTransfer.files) addFiles(e.dataTransfer.files); }}
        >
          <div className="rs-dropzone-ico"><Icons.upload size={22} /></div>
          <strong>Drag &amp; drop files here</strong>
          <span>or click to browse</span>
        </div>

        {staged.length > 0 && (
          <>
            <div className="rs-media-sectionhead" style={{ marginTop: 16 }}>
              <h2>Ready to upload</h2>
              <span className="rs-count-pill">{staged.length}</span>
              <div className="rs-spacer" />
              <button className="rs-link-btn rs-danger" onClick={() => setStaged([])} type="button">Clear all</button>
            </div>
            <div className="rs-stage-list">
              {staged.map((s) => (
                <div className="rs-stage-row" key={s.sid}>
                  <div className="rs-stage-thumb"><span className="rs-mono">{(s.file.name.split(".").pop() || "file").toUpperCase()}</span></div>
                  <div className="rs-stage-meta">
                    <strong title={s.file.name}>{s.file.name}</strong>
                    <span className="rs-mono">{(s.file.size / 1048576).toFixed(1)} MB</span>
                  </div>
                  <button className="rs-row-btn" onClick={() => setStaged((p) => p.filter((x) => x.sid !== s.sid))} type="button"><Icons.x size={16} /></button>
                </div>
              ))}
            </div>
          </>
        )}
      </div>

      <div className="rs-modal-foot">
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <div className="rs-spacer" />
        <label style={{ display: "flex", alignItems: "center", gap: 8, marginRight: 4 }}>
          <span style={{ fontSize: 12.5, color: "var(--text-muted)" }}>Folder</span>
          <select
            className="rs-input rs-input--sm"
            value={dest ?? ""}
            onChange={(e) => setDest(e.target.value === "" ? null : e.target.value)}
          >
            <option value="">Media Library</option>
            {folders.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
          </select>
        </label>
        <button className="rs-btn rs-btn--primary" disabled={!staged.length || busy} onClick={upload} type="button">
          <Icons.check size={15} /> {busy ? "Uploading…" : `Upload ${staged.length || ""} asset${staged.length === 1 ? "" : "s"}`}
        </button>
      </div>
    </>
  );
}
