import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

const STORAGE_KEY = 'malstrom.liveDbFolder';

// Persists the user-chosen Live Database folder across launches. Ableton
// keeps one Live-files-*.db per installed Live major version, so a folder
// (not a single file) lets categorization try every version's tags. No
// filesystem search of any kind -- the user always picks the folder via a
// native dialog; this just remembers their last choice.
export function useLiveDbFolder() {
  const [liveDbFolder, setLiveDbFolderState] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY),
  );

  function setLiveDbFolder(path: string | null) {
    setLiveDbFolderState(path);
    if (path) {
      localStorage.setItem(STORAGE_KEY, path);
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  }

  async function chooseLiveDbFolder() {
    const selected = await open({ directory: true });
    if (typeof selected === 'string') {
      setLiveDbFolder(selected);
    }
  }

  return { liveDbFolder, chooseLiveDbFolder, clearLiveDbFolder: () => setLiveDbFolder(null) };
}
