import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import {
  listFolders, createFolder, updateFolder, deleteFolder,
  listAssets, updateAsset, deleteAsset, uploadAsset,
} from "../api/endpoints";
import { ApiError } from "../api/client";
import type { MediaFolder, MediaAsset } from "../api/types";
import { Notice } from "../components/ui";
import { Checkbox } from "./media/Checkbox";
import { AssetThumb } from "./media/AssetThumb";
import { FolderModal } from "./media/FolderModal";
import { MoveModal } from "./media/MoveModal";
import { UploadModal } from "./media/UploadModal";
import { AssetDetail } from "./media/AssetDetail";

type ModalState = null | "folder" | "upload" | "move" | { editFolder: MediaFolder };
type Sort = "newest" | "oldest" | "name";

export function MediaLibrary() {
  const [folders, setFolders] = useState<MediaFolder[]>([]);
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [cur, setCur] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("newest");
  const [selected, setSelected] = useState<string[]>([]);
  const [modal, setModal] = useState<ModalState>(null);
  const [detail, setDetail] = useState<MediaAsset | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const navigate = useNavigate();
  const dragIds = useRef<string[]>([]);

  const reloadFolders = useCallback(async () => {
    setFolders(await listFolders({ all: true }));
  }, []);
  const reloadAssets = useCallback(async (folderId: string | null) => {
    setAssets(await listAssets(folderId));
  }, []);

  useEffect(() => { reloadFolders().catch(() => {}); }, [reloadFolders]);
  useEffect(() => { reloadAssets(cur).catch(() => {}); }, [cur, reloadAssets]);

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
  const folderCount = (id: string) => folders.filter((f) => f.parent_id === id).length;
  const totalMb = (assets.reduce((n, a) => n + a.size_bytes, 0) / 1048576).toFixed(1);

  let visible = assets.slice();
  if (query) visible = visible.filter((a) => a.file_name.toLowerCase().includes(query.toLowerCase()));
  visible.sort((a, b) =>
    sort === "name" ? a.file_name.localeCompare(b.file_name)
      : sort === "oldest" ? a.created_at.localeCompare(b.created_at)
        : b.created_at.localeCompare(a.created_at));

  const flash = (msg: string) => { setNotice(msg); setTimeout(() => setNotice(null), 4000); };

  const onCreateFolder = async (data: { name: string; parent_id: string | null }) => {
    await createFolder({ name: data.name, parent_id: data.parent_id });
    await reloadFolders();
    setModal(null);
  };
  const onEditFolder = async (id: string, data: { name: string; parent_id: string | null }) => {
    await updateFolder(id, { name: data.name, parent_id: data.parent_id });
    await reloadFolders();
    setModal(null);
  };
  const onDeleteFolder = async (id: string) => {
    if (!window.confirm("Delete this folder?")) return;
    try {
      await deleteFolder(id);
      await reloadFolders();
      if (cur === id) setCur(folders.find((f) => f.id === id)?.parent_id ?? null);
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        flash("Folder not empty — move or delete its contents first.");
      } else {
        flash(e instanceof Error ? e.message : "Could not delete folder.");
      }
    }
  };
  const onUpload = async (files: File[], dest: string | null) => {
    let ok = 0, failed = 0;
    for (const f of files) {
      try { await uploadAsset(f, dest); ok++; } catch { failed++; }
    }
    setModal(null);
    if (dest !== cur) setCur(dest); else await reloadAssets(cur);
    if (failed) flash(`Uploaded ${ok}, ${failed} failed.`);
    return { ok, failed };
  };
  const moveAssets = async (ids: string[], dest: string | null) => {
    for (const id of ids) await updateAsset(id, { folder_id: dest });
    setSelected([]);
    setModal(null);
    await reloadAssets(cur);
  };
  const onDeleteAssets = async (ids: string[]) => {
    if (!window.confirm(`Delete ${ids.length} asset${ids.length === 1 ? "" : "s"}?`)) return;
    for (const id of ids) await deleteAsset(id);
    setSelected([]);
    await reloadAssets(cur);
  };
  const onSaveAsset = async (id: string, patch: Parameters<typeof updateAsset>[1]) => {
    await updateAsset(id, patch);
    setDetail(null);
    await reloadAssets(cur);
  };
  const onDeleteAsset = async (id: string) => {
    await deleteAsset(id);
    setDetail(null);
    await reloadAssets(cur);
  };

  const toggleSel = (id: string) =>
    setSelected((s) => s.includes(id) ? s.filter((x) => x !== id) : [...s, id]);

  const onAssetDragStart = (id: string) => { dragIds.current = selected.includes(id) ? selected : [id]; };
  const onFolderDrop = (folderId: string) => {
    if (dragIds.current.length && folderId !== cur) moveAssets(dragIds.current, folderId);
    setDropTarget(null);
    dragIds.current = [];
  };

  const isEmpty = subFolders.length === 0 && visible.length === 0;

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Media Library</h1>
          <p className="rs-cm-sub">{assets.length} assets · {folders.length} folders · {totalMb} MB</p>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" title="Media storage settings" onClick={() => navigate("/settings/media")} type="button"><Icons.gear size={16} /> Settings</button>
          <button className="rs-btn rs-btn--ghost" onClick={() => setModal("folder")} type="button"><Icons.folderPlus size={16} /> Add new folder</button>
          <button className="rs-btn rs-btn--primary" onClick={() => setModal("upload")} type="button"><Icons.upload size={16} /> Add new assets</button>
        </div>
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

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input placeholder="Search assets" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--ghost" type="button"
          onClick={() => setSort(sort === "newest" ? "oldest" : sort === "oldest" ? "name" : "newest")}>
          <Icons.sort size={15} /> {sort === "name" ? "Name (A–Z)" : sort === "oldest" ? "Oldest first" : "Newest first"}
        </button>
      </div>

      {notice && <Notice>{notice}</Notice>}

      {selected.length > 0 && (
        <div className="rs-bulkbar">
          <span><strong>{selected.length}</strong> selected</span>
          <div className="rs-bulkbar-actions">
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setModal("move")} type="button"><Icons.folderInput size={14} /> Move to folder</button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm rs-danger" onClick={() => onDeleteAssets(selected)} type="button"><Icons.trash size={14} /> Delete</button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setSelected([])} type="button">Clear</button>
          </div>
        </div>
      )}

      {isEmpty ? (
        <div className="rs-media-empty">
          <div className="rs-media-empty-ico"><Icons.image size={24} /></div>
          <h3>{query ? "No assets match your search" : "This folder is empty"}</h3>
          <p>{query ? "Try a different search term." : "Upload assets or create a folder to start organizing your media."}</p>
          {!query && <div className="rs-editor-actions" style={{ marginTop: 10 }}>
            <button className="rs-btn rs-btn--ghost" onClick={() => setModal("folder")} type="button"><Icons.folderPlus size={16} /> New folder</button>
            <button className="rs-btn rs-btn--primary" onClick={() => setModal("upload")} type="button"><Icons.upload size={16} /> Add assets</button>
          </div>}
        </div>
      ) : (
        <>
          {subFolders.length > 0 && (
            <>
              <div className="rs-media-sectionhead"><h2>Folders</h2><span className="rs-count-pill">{subFolders.length}</span></div>
              <div className="rs-folder-grid">
                {subFolders.map((f) => (
                  <div key={f.id}
                    className={"rs-folder-card" + (dropTarget === f.id ? " is-drop" : "")}
                    onClick={() => setCur(f.id)}
                    onDragOver={(e) => { if (dragIds.current.length) { e.preventDefault(); setDropTarget(f.id); } }}
                    onDragLeave={() => setDropTarget((d) => d === f.id ? null : d)}
                    onDrop={(e) => { e.preventDefault(); onFolderDrop(f.id); }}>
                    <span className="rs-folder-ico"><Icons.folder size={22} /></span>
                    <span className="rs-folder-meta">
                      <strong title={f.name}>{f.name}</strong>
                      <span>{folderCount(f.id)} folder{folderCount(f.id) === 1 ? "" : "s"}</span>
                    </span>
                    <span className="rs-folder-menu" title="Edit folder"
                      onClick={(e) => { e.stopPropagation(); setModal({ editFolder: f }); }}><Icons.edit size={16} /></span>
                    <span className="rs-folder-menu" title="Delete folder"
                      onClick={(e) => { e.stopPropagation(); onDeleteFolder(f.id); }}><Icons.trash size={16} /></span>
                  </div>
                ))}
              </div>
            </>
          )}

          {visible.length > 0 && (
            <>
              <div className="rs-media-sectionhead">
                <h2>Assets</h2><span className="rs-count-pill">{visible.length}</span>
                <div className="rs-spacer" />
                <button className="rs-link-btn" type="button"
                  onClick={() => setSelected(selected.length === visible.length ? [] : visible.map((a) => a.id))}>
                  {selected.length === visible.length ? "Deselect all" : "Select all"}
                </button>
              </div>
              <div className="rs-media-grid">
                {visible.map((m) => {
                  const sel = selected.includes(m.id);
                  return (
                    <div className={"rs-media-card" + (sel ? " is-selected" : "")} key={m.id}
                      draggable onDragStart={() => onAssetDragStart(m.id)}
                      onClick={() => setDetail(m)}>
                      <div className="rs-media-check" onClick={(e) => { e.stopPropagation(); toggleSel(m.id); }}>
                        <Checkbox checked={sel} onChange={() => toggleSel(m.id)} />
                      </div>
                      <AssetThumb asset={m} />
                      <div className="rs-media-card-meta">
                        <span className="rs-media-card-text">
                          <strong title={m.file_name}>{m.file_name}</strong>
                          <span className="rs-cell-muted rs-mono">
                            {m.width && m.height ? `${m.width}×${m.height} · ` : ""}{(m.size_bytes / 1048576).toFixed(1)} MB
                          </span>
                        </span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </>
      )}

      {modal === "folder" && (
        <FolderModal folders={folders} currentFolder={cur} onClose={() => setModal(null)} onSubmit={onCreateFolder} />
      )}
      {modal && typeof modal === "object" && "editFolder" in modal && (
        <FolderModal folders={folders} currentFolder={cur} editing={modal.editFolder}
          onClose={() => setModal(null)} onSubmit={(d) => onEditFolder(modal.editFolder.id, d)} />
      )}
      {modal === "upload" && (
        <UploadModal folders={folders} currentFolder={cur} onClose={() => setModal(null)} onUpload={onUpload} />
      )}
      {modal === "move" && (
        <MoveModal folders={folders} count={selected.length} onClose={() => setModal(null)} onMove={(dest) => moveAssets(selected, dest)} />
      )}
      {detail && (
        <AssetDetail asset={detail} onClose={() => setDetail(null)}
          onSave={(patch) => onSaveAsset(detail.id, patch)} onDelete={() => onDeleteAsset(detail.id)} />
      )}
    </div>
  );
}
