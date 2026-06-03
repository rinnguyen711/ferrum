import { useState, useRef } from "react";
import { Modal } from "./Modal";
import { Icons } from "../../components/icons";
import type { MediaFolder } from "../../api/types";

interface Staged { file: File; sid: string; status: "ready" | "failed"; }

let _seq = 0;

export function UploadModal({
  folders, currentFolder, onClose, onUpload,
}: {
  folders: MediaFolder[];
  currentFolder: string | null;
  onClose: () => void;
  onUpload: (files: File[], dest: string | null) => Promise<{ ok: number; failed: number }>;
}) {
  const [staged, setStaged] = useState<Staged[]>([]);
  const [dest, setDest] = useState<string | null>(currentFolder);
  const [over, setOver] = useState(false);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const addFiles = (files: FileList | File[]) => {
    const next = Array.from(files).map((file) => ({ file, sid: "s" + (++_seq), status: "ready" as const }));
    setStaged((s) => [...s, ...next]);
  };
  const removeStaged = (sid: string) => setStaged((s) => s.filter((x) => x.sid !== sid));

  const folderName = (id: string | null) =>
    id == null ? "Media Library" : (folders.find((f) => f.id === id)?.name ?? "Media Library");

  const upload = async () => {
    if (!staged.length || busy) return;
    setBusy(true);
    try {
      await onUpload(staged.map((s) => s.file), dest);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      wide
      eyebrow={"Upload to " + folderName(dest)}
      title="Add new assets"
      icon="upload"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <div className="rs-spacer" />
        <label style={{ display: "flex", alignItems: "center", marginRight: 8 }}>
          <span className="rs-cell-muted" style={{ fontSize: 12.5, marginRight: 8 }}>Destination</span>
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
          <Icons.check size={15} /> Upload {staged.length || ""} asset{staged.length === 1 ? "" : "s"}
        </button>
      </>}
    >
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
          <div className="rs-media-sectionhead" style={{ marginTop: 18 }}>
            <h2>Ready to upload</h2><span className="rs-count-pill">{staged.length}</span>
            <div className="rs-spacer" />
            <button className="rs-link-btn rs-danger" onClick={() => setStaged([])} type="button">Clear all</button>
          </div>
          <div className="rs-stage-list">
            {staged.map((s) => (
              <div className="rs-stage-row" key={s.sid}>
                <div className="rs-stage-thumb"><span className="rs-mono">{(s.file.name.split(".").pop() || "file").toUpperCase()}</span></div>
                <div className="rs-stage-meta">
                  <strong title={s.file.name}>{s.file.name}{s.status === "failed" ? " — failed" : ""}</strong>
                  <span className="rs-mono">{(s.file.size / 1048576).toFixed(1)} MB</span>
                </div>
                <button className="rs-row-btn" onClick={() => removeStaged(s.sid)} type="button"><Icons.x size={16} /></button>
              </div>
            ))}
          </div>
        </>
      )}
    </Modal>
  );
}
