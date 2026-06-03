import { useState } from "react";
import { Modal } from "./Modal";
import { AssetThumb } from "./AssetThumb";
import { Icons } from "../../components/icons";
import type { MediaAsset, PatchAsset } from "../../api/types";

export function AssetDetail({
  asset, onClose, onSave, onDelete,
}: {
  asset: MediaAsset;
  onClose: () => void;
  onSave: (patch: PatchAsset) => Promise<void>;
  onDelete: () => Promise<void>;
}) {
  const [fileName, setFileName] = useState(asset.file_name);
  const [alt, setAlt] = useState(asset.alt_text ?? "");
  const [caption, setCaption] = useState(asset.caption ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const save = async () => {
    if (busy || !fileName.trim()) return;
    setBusy(true); setError(null);
    try {
      await onSave({ file_name: fileName.trim(), alt_text: alt, caption });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save.");
      setBusy(false);
    }
  };

  const remove = async () => {
    if (busy) return;
    if (!window.confirm(`Delete "${asset.file_name}"? This cannot be undone.`)) return;
    setBusy(true);
    try { await onDelete(); } catch (e) {
      setError(e instanceof Error ? e.message : "Could not delete.");
      setBusy(false);
    }
  };

  const dims = asset.width && asset.height ? `${asset.width}×${asset.height} · ` : "";
  const sizeMb = (asset.size_bytes / 1048576).toFixed(1) + " MB";

  return (
    <Modal
      wide
      eyebrow="Asset"
      title={asset.file_name}
      icon="image"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost rs-danger" onClick={remove} type="button"><Icons.trash size={15} /> Delete</button>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <button className="rs-btn rs-btn--primary" disabled={busy || !fileName.trim()} onClick={save} type="button">
          <Icons.check size={15} /> Save
        </button>
      </>}
    >
      {error && <div className="rs-login-error" style={{ marginBottom: 12 }}>{error}</div>}
      <div className="rs-asset-detail">
        <AssetThumb asset={asset} className="rs-asset-detail-preview" />
        <p className="rs-cell-muted rs-mono" style={{ marginTop: 8 }}>{asset.mime_type} · {dims}{sizeMb}</p>
        <div className="rs-fields" style={{ marginTop: 12 }}>
          <div className="rs-field">
            <div className="rs-field-label"><label>File name</label></div>
            <input className="rs-input" value={fileName} onChange={(e) => setFileName(e.target.value)} />
          </div>
          <div className="rs-field">
            <div className="rs-field-label"><label>Alternative text</label><span className="rs-field-hint">For accessibility</span></div>
            <input className="rs-input" value={alt} onChange={(e) => setAlt(e.target.value)} placeholder="Describe the image" />
          </div>
          <div className="rs-field">
            <div className="rs-field-label"><label>Caption</label></div>
            <textarea className="rs-input rs-textarea" rows={2} value={caption} onChange={(e) => setCaption(e.target.value)} />
          </div>
        </div>
      </div>
    </Modal>
  );
}
