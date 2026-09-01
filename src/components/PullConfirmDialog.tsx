import type { PendingPull } from '../types/scaleMode';
import { Modal } from './Modal';

export function PullConfirmDialog({
  pending,
  onCancel,
  onConfirm,
}: {
  pending: PendingPull;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Modal onClose={onCancel} labelledBy="pull-confirm-title">
      <h3 id="pull-confirm-title">
        Pull {pending.root_name} {pending.scale_name} into {pending.projectName}?
      </h3>
      <p>This overwrites {pending.projectName} in place.</p>
      <div className="pull-confirm-actions">
        <button type="button" className="filter-toggle active" onClick={onConfirm}>
          Overwrite in place
        </button>
        <button type="button" className="filter-toggle" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </Modal>
  );
}
