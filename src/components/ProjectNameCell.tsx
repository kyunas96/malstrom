import { openPath } from '@tauri-apps/plugin-opener';
import { colorForTagIndex } from '../lib/tagColors';
import type { ActiveScaleTag } from '../types/scaleFilter';

export function ProjectNameCell({
  name,
  path,
  pinnedPaths,
  activeTags,
}: {
  name: string;
  path: string;
  pinnedPaths: string[];
  activeTags: ActiveScaleTag[];
}) {
  const isPinned = pinnedPaths.includes(path);
  const tagIndex = isPinned ? activeTags.findIndex((t) => t.originPath === path) : -1;
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
    </span>
  );
}
