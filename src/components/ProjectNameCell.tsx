import { useState } from 'react';
import { openPath } from '@tauri-apps/plugin-opener';
import { invoke } from '@tauri-apps/api/core';
import { colorForTagIndex } from '../lib/tagColors';
import type { ActiveScaleTag } from '../types/scaleFilter';

type TrackSummary = { name: string; kind: string; category: string };

export function ProjectNameCell({
  name,
  path,
  pinnedPaths,
  activeTags,
  liveDbPath,
}: {
  name: string;
  path: string;
  pinnedPaths: string[];
  activeTags: ActiveScaleTag[];
  liveDbPath: string | null;
}) {
  const isPinned = pinnedPaths.includes(path);
  const tagIndex = isPinned ? activeTags.findIndex((t) => t.originPath === path) : -1;
  const [tracks, setTracks] = useState<TrackSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function loadTracks() {
    if (tracks) {
      setTracks(null);
      return;
    }
    setError(null);
    try {
      const result = await invoke<TrackSummary[]>('list_tracks', {
        path,
        liveDbPath: liveDbPath ?? undefined,
      });
      setTracks(result);
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <span title={path}>
      {name}
      {isPinned && tagIndex >= 0 && (
        <span
          className="pinned-icon"
          style={{ color: colorForTagIndex(tagIndex).fg }}
          title="Pinned — source of filter"
        >
          📌
        </span>
      )}
      <button
        type="button"
        className="open-in-live-button"
        onClick={() => openPath(path).catch((err) => console.error(err))}
      >
        Open in Ableton Live
      </button>
      <button type="button" className="open-in-live-button" onClick={loadTracks}>
        {tracks ? 'Hide Track Tags' : 'Show Track Tags'}
      </button>
      {error && <div style={{ color: 'red' }}>{error}</div>}
      {tracks && (
        <ul>
          {tracks.map((t, i) => (
            <li key={i}>
              {t.name} ({t.kind}) — {t.category}
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}
