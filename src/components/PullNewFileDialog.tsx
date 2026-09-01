import { useState } from 'react';
import type { PendingNewFilePull } from '../types/scaleMode';
import { collidesWithOriginal } from '../lib/fileName';
import { Modal } from './Modal';

export function PullNewFileDialog({
  pending,
  onCancel,
  onConfirm,
}: {
  pending: PendingNewFilePull;
  onCancel: () => void;
  onConfirm: (fileName: string) => void;
}) {
  const [fileName, setFileName] = useState(pending.defaultFileName);
  const collides = collidesWithOriginal(fileName, pending.projectPath);
  const canSave = fileName.trim().length > 0 && !collides;

  function confirm() {
    if (!canSave) return;
    onConfirm(fileName);
  }

  return (
    <Modal onClose={onCancel} labelledBy="pull-new-file-title">
      <h3 id="pull-new-file-title">
        Pull {pending.root_name} {pending.scale_name} into {pending.projectName}?
      </h3>
      <p>This is saved as a new file — the original project is never touched.</p>
      <input
        type="text"
        className="pull-new-file-input"
        value={fileName}
        autoFocus
        onFocus={(e) => e.target.select()}
        onChange={(e) => setFileName(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && confirm()}
      />
      {collides && (
        <p className="pull-new-file-warning">
          That name matches the original file — choose a different name to save as a new file.
        </p>
      )}
      <div className="pull-confirm-actions">
        <button type="button" className="filter-toggle active" onClick={confirm} disabled={!canSave}>
          Save
        </button>
        <button type="button" className="filter-toggle" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </Modal>
  );
}
