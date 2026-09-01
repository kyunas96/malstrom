import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

const STORAGE_KEY = 'malstrom.liveDbPath';

// Persists the user-chosen Live Database path across launches. No
// filesystem search of any kind -- the user always picks the file via a
// native dialog; this just remembers their last choice.
export function useLiveDbPath() {
  const [liveDbPath, setLiveDbPathState] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY),
  );

  function setLiveDbPath(path: string | null) {
    setLiveDbPathState(path);
    if (path) {
      localStorage.setItem(STORAGE_KEY, path);
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  }

  async function chooseLiveDbFile() {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Live Database', extensions: ['db'] }],
    });
    if (typeof selected === 'string') {
      setLiveDbPath(selected);
    }
  }

  return { liveDbPath, chooseLiveDbFile, clearLiveDbPath: () => setLiveDbPath(null) };
}
