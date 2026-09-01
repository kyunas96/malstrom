import { useState } from 'react';
import { openPath } from '@tauri-apps/plugin-opener';
import { invoke } from '@tauri-apps/api/core';
import { colorForTagIndex } from '../lib/tagColors';
import type { ActiveScaleTag } from '../types/scaleFilter';

type TrackSummary = { name: string; kind: string; category: string };

const TRACK_CATEGORIES = [
  'Drums',
  'Bass',
  'Percussion',
  'Vocals',
  'Lead',
  'Pads',
  'Fx',
  'Other',
];

export function ProjectNameCell({
  name,
  path,
  pinnedPaths,
  activeTags,
  liveDbFolder,
}: {
  name: string;
  path: string;
  pinnedPaths: string[];
  activeTags: ActiveScaleTag[];
  liveDbFolder: string | null;
}) {
  const isPinned = pinnedPaths.includes(path);
  const tagIndex = isPinned ? activeTags.findIndex((t) => t.originPath === path) : -1;
  const [tracks, setTracks] = useState<TrackSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // ponytail: "(overridden)" only reflects changes made this session, not
  // overrides set in a prior session -- upgrade to a bulk overlay read if
  // that distinction turns out to matter.
  const [overriddenNames, setOverriddenNames] = useState<Set<string>>(new Set());

  async function loadTracks() {
    if (tracks) {
      setTracks(null);
      return;
    }
    setError(null);
    try {
      const result = await invoke<TrackSummary[]>('list_tracks', {
        path,
        liveDbFolder: liveDbFolder ?? undefined,
      });
      setTracks(result);
    } catch (err) {
      setError(String(err));
    }
  }

  async function setCategoryOverride(trackName: string, category: string) {
    await invoke('overlay_set', {
      namespace: 'trackCategoryOverrides',
      key: `${path}::${trackName}`,
      value: category,
    });
    setOverriddenNames((prev) => new Set(prev).add(trackName));
    setTracks(
      (prev) =>
        prev?.map((t) => (t.name === trackName ? { ...t, category } : t)) ?? prev
    );
  }

  async function resetCategoryOverride(trackName: string) {
    await invoke('overlay_remove', {
      namespace: 'trackCategoryOverrides',
      key: `${path}::${trackName}`,
    });
    setOverriddenNames((prev) => {
      const next = new Set(prev);
      next.delete(trackName);
      return next;
    });
    const result = await invoke<TrackSummary[]>('list_tracks', {
      path,
      liveDbFolder: liveDbFolder ?? undefined,
    });
    setTracks(result);
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
              {t.name} ({t.kind}){' '}
              <select
                value={t.category}
                onChange={(e) => setCategoryOverride(t.name, e.target.value)}
              >
                {TRACK_CATEGORIES.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
              {overriddenNames.has(t.name) && (
                <>
                  {' '}
                  <span>(overridden)</span>{' '}
                  <button type="button" onClick={() => resetCategoryOverride(t.name)}>
                    Reset
                  </button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}
