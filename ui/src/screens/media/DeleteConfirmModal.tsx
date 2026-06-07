import { Modal } from "./Modal";
import { Icons } from "../../components/icons";

export function DeleteConfirmModal({
  message,
  onConfirm,
  onCancel,
}: {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal
      eyebrow="Confirm"
      title="Delete"
      icon="trash"
      onClose={onCancel}
      footer={<>
        <button className="rs-btn rs-btn--ghost" onClick={onCancel} type="button">Cancel</button>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--primary" onClick={onConfirm} type="button">
          <Icons.trash size={15} /> Delete
        </button>
      </>}
    >
      <p>{message}</p>
    </Modal>
  );
}
