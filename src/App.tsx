import { useState } from 'react';
import { AlsProjectList } from './AlsProjectList';
import { SettingsPanel } from './components/SettingsPanel';
import { useLiveDbFolder } from './hooks/useLiveDbFolder';
import './App.css';

function App() {
  const { liveDbFolder, chooseLiveDbFolder, clearLiveDbFolder } = useLiveDbFolder();
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <main className="container">
      <button
        type="button"
        className="settings-gear-button"
        onClick={() => setSettingsOpen(true)}
        aria-label="Settings"
      >
        ⚙️
      </button>
      <AlsProjectList liveDbFolder={liveDbFolder} />
      {settingsOpen && (
        <SettingsPanel
          onClose={() => setSettingsOpen(false)}
          liveDbFolder={liveDbFolder}
          onChooseFolder={chooseLiveDbFolder}
          onClearFolder={clearLiveDbFolder}
        />
      )}
    </main>
  );
}

export default App;
