import { useState, useEffect, useRef } from "react";
import { Modal } from "./Modal";
import { Icons } from "../../components/icons";
import { Notice } from "../../components/ui";
import type { MediaFolder } from "../../api/types";

export function FolderModal({
  folders, currentFolder, editing, onClose, onSubmit,
}: {
  folders: MediaFolder[];
  currentFolder: string | null;
  editing?: MediaFolder;
  onClose: () => void;
  onSubmit: (data: { name: string; parent_id: string | null }) => Promise<void>;
}) {
  const [name, setName] = useState(editing ? editing.name : "");
  const [parent, setParent] = useState<string | null>(editing ? editing.parent_id : currentFolder);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { ref.current?.focus(); }, []);

  const valid = name.trim().length > 0;
  const parentOptions = folders.filter((f) => !editing || f.id !== editing.id);

  const submit = async () => {
    if (!valid || busy) return;
    setBusy(true); setError(null);
    try {
      await onSubmit({ name: name.trim(), parent_id: parent });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save folder.");
      setBusy(false);
    }
  };

  return (
    <Modal
      eyebrow={editing ? "Edit folder" : "New folder"}
      title={name.trim() || "Untitled folder"}
      icon="folderPlus"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <button className="rs-btn rs-btn--primary" disabled={!valid || busy} onClick={submit} type="button">
          <Icons.check size={15} /> {editing ? "Save folder" : "Create folder"}
        </button>
      </>}
    >
      {error && <Notice>{error}</Notice>}
      <div className="rs-fields">
        <div className="rs-field">
          <div className="rs-field-label"><label>Name</label><span className="rs-field-hint">Shown in the media library</span></div>
          <input
            ref={ref}
            className="rs-input"
            value={name}
            placeholder="e.g. Article covers"
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
        </div>
        <div className="rs-field">
          <div className="rs-field-label"><label>Location</label></div>
          <select
            className="rs-input"
            value={parent ?? ""}
            onChange={(e) => setParent(e.target.value === "" ? null : e.target.value)}
          >
            <option value="">Media Library (root)</option>
            {parentOptions.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
          </select>
        </div>
      </div>
    </Modal>
  );
}
