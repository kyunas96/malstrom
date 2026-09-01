import { Modal } from './Modal';

export function SettingsPanel({
  onClose,
  liveDbFolder,
  onChooseFolder,
  onClearFolder,
}: {
  onClose: () => void;
  liveDbFolder: string | null;
  onChooseFolder: () => void;
  onClearFolder: () => void;
}) {
  return (
    <Modal onClose={onClose} labelledBy="settings-panel-title">
      <h3 id="settings-panel-title">Settings</h3>
      <p className="settings-db-path" title={liveDbFolder ?? undefined}>
        Live Database folder: {liveDbFolder ?? 'Not set'}
      </p>
      <div className="pull-confirm-actions">
        <button type="button" className="filter-toggle active" onClick={onChooseFolder}>
          Choose Live Database Folder…
        </button>
        {liveDbFolder && (
          <button type="button" className="filter-toggle" onClick={onClearFolder}>
            Clear
          </button>
        )}
        <button type="button" className="filter-toggle" onClick={onClose}>
          Close
        </button>
      </div>
    </Modal>
  );
}
