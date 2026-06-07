import { useCallback, useEffect, useMemo, useState } from "react";
import { Icons } from "../../components/icons";
import { listFolders, listAssets } from "../../api/endpoints";
import type { MediaFolder, MediaAsset } from "../../api/types";
import { AssetThumb } from "./AssetThumb";
import { SelectBox } from "./SelectBox";

export function AssetPicker({
  multiple,
  onClose,
  onPick,
}: {
  multiple: boolean;
  onClose: () => void;
  onPick: (assets: MediaAsset[]) => void;
}) {
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
          <button className="rs-modal-x" onClick={onClose}><Icons.x size={18} /></button>
        </div>

        <div className="rs-media-bc">
          {path.length === 0
            ? <span className="rs-media-bc-here">Media Library</span>
            : <button onClick={() => setCur(null)} type="button">Media Library</button>}
          {path.map((f, i) => (
            <span key={f.id} style={{ display: "contents" }}>
              <span className="rs-media-bc-sep">/</span>
              {i === path.length - 1
                ? <span className="rs-media-bc-here">{f.name}</span>
                : <button onClick={() => setCur(f.id)} type="button">{f.name}</button>}
            </span>
          ))}
        </div>

        <div className="rs-modal-body">
          {subFolders.length > 0 && (
            <div className="rs-folder-grid">
              {subFolders.map((f) => (
                <div key={f.id} className="rs-folder-card" onClick={() => setCur(f.id)}>
                  <span className="rs-folder-ico"><Icons.folder size={22} /></span>
                  <span className="rs-folder-meta"><strong title={f.name}>{f.name}</strong></span>
                </div>
              ))}
            </div>
          )}
          {assets.length === 0 ? (
            <div className="rs-media-empty"><p>No assets in this folder.</p></div>
          ) : (
            <div className="rs-media-grid">
              {assets.map((m) => {
                const sel = pickedIds.has(m.id);
                return (
                  <div className={"rs-media-card" + (sel ? " is-selected" : "")} key={m.id} onClick={() => toggle(m)}>
                    <div className="rs-media-check" onClick={(e) => { e.stopPropagation(); toggle(m); }}>
                      <SelectBox checked={sel} />
                    </div>
                    <AssetThumb asset={m} />
                    <div className="rs-media-card-meta">
                      <span className="rs-media-card-text"><strong title={m.file_name}>{m.file_name}</strong></span>
                    </div>
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
      </div>
    </div>
  );
}
