import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import type { PatchContentType } from "../api/types";
import { useBuilderDraft } from "./BuilderDraftContext";
import { diffToPatch } from "./draftModel";
import { SaveConfirmModal } from "./SaveConfirmModal";
import { ConfirmDialog } from "../components/ui";

/** Floating dirty-state save bar — the single save point for the Builder
 *  section. Renders nothing while the draft matches the server. */
export function SaveBar({ disabled = false }: { disabled?: boolean }) {
  const { draft, dirty, saving, save, discard, reset } = useBuilderDraft();
  const navigate = useNavigate();
  const [confirmPatch, setConfirmPatch] = useState<PatchContentType | null>(null);
  const [confirmDiscardNew, setConfirmDiscardNew] = useState(false);

  if (!draft || (!dirty && !saving)) return null;

  const onSave = () => {
    if (draft.mode === "existing") {
      const patch = diffToPatch(draft);
      if (patch.drop_fields.length > 0) {
        setConfirmPatch(patch);
        return;
      }
    }
    void save();
  };

  const onDiscard = () => {
    if (draft.mode === "new") {
      setConfirmDiscardNew(true);
      return;
    }
    discard();
  };

  const noun = draft.mode === "component" ? "component" : "schema";

  return (
    <>
      <div className="rs-savebar" role="status">
        <span className="rs-savebar-msg">
          <span className="rs-dot" /> Unsaved {noun} changes
        </span>
        <div className="rs-savebar-actions">
          <button className="rs-btn rs-btn--ghost" onClick={onDiscard} disabled={saving || disabled}>
            Discard
          </button>
          <button className="rs-btn rs-btn--primary" onClick={onSave} disabled={saving || disabled}>
            {saving
              ? <><Icons.spinner size={14} className="rs-spin" /> Saving…</>
              : <><Icons.save size={14} /> Save</>}
          </button>
        </div>
      </div>
      {confirmPatch && (
        <SaveConfirmModal
          patch={confirmPatch}
          saving={saving}
          onConfirm={async () => {
            await save();
            setConfirmPatch(null);
          }}
          onCancel={() => setConfirmPatch(null)}
        />
      )}
      {confirmDiscardNew && (
        <ConfirmDialog
          title="Discard this unsaved type?"
          body="This type hasn't been created yet. Discarding loses your work."
          confirmLabel="Discard type"
          onConfirm={() => {
            setConfirmDiscardNew(false);
            reset();
            navigate("/builder");
          }}
          onCancel={() => setConfirmDiscardNew(false)}
        />
      )}
    </>
  );
}
