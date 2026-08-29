import type { PendingPull } from '../types/scaleMode';

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
    <div className="pull-confirm-backdrop" onClick={onCancel}>
      <div
        className="pull-confirm-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pull-confirm-title"
        onClick={(e) => e.stopPropagation()}
      >
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
      </div>
    </div>
  );
}
