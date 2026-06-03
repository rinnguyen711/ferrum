import { useState } from "react";
import { Modal } from "./Modal";
import { Icons } from "../../components/icons";
import type { MediaFolder } from "../../api/types";

export function MoveModal({
  folders, count, onClose, onMove,
}: {
  folders: MediaFolder[];
  count: number;
  onClose: () => void;
  onMove: (dest: string | null) => Promise<void>;
}) {
  const [dest, setDest] = useState<string | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const roots = folders.filter((f) => f.parent_id == null);
  const childrenOf = (id: string) => folders.filter((f) => f.parent_id === id);

  const Row = ({ f, nested }: { f: MediaFolder; nested?: boolean }) => (
    <button
      type="button"
      className={"rs-foldpick-item" + (nested ? " is-nested" : "") + (dest === f.id ? " is-on" : "")}
      onClick={() => setDest(f.id)}
    >
      <span className="rs-folder-ico"><Icons.folder size={17} /></span>
      <strong>{f.name}</strong>
      <span className="rs-radio-dot" />
    </button>
  );

  const confirm = async () => {
    if (dest === undefined || busy) return;
    setBusy(true);
    try { await onMove(dest); } finally { setBusy(false); }
  };

  return (
    <Modal
      eyebrow={count + " asset" + (count === 1 ? "" : "s") + " selected"}
      title="Move to folder"
      icon="folderInput"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <button className="rs-btn rs-btn--primary" disabled={dest === undefined || busy} onClick={confirm} type="button">
          <Icons.folderInput size={15} /> Move here
        </button>
      </>}
    >
      <div className="rs-foldpick">
        <button
          type="button"
          className={"rs-foldpick-item" + (dest === null ? " is-on" : "")}
          onClick={() => setDest(null)}
        >
          <span className="rs-folder-ico"><Icons.home size={17} /></span>
          <strong>Media Library (root)</strong>
          <span className="rs-radio-dot" />
        </button>
        {roots.map((f) => (
          <div key={f.id}>
            <Row f={f} />
            {childrenOf(f.id).map((c) => <Row key={c.id} f={c} nested />)}
          </div>
        ))}
      </div>
    </Modal>
  );
}
