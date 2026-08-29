import { useState } from 'react';
import type { ScaleCandidate } from '../types/alsProject';
import type { ActiveScaleTag } from '../types/scaleFilter';
import type { ApplyState } from '../types/scaleMode';
import { groupByRootNote, candidateMatchesTag, ROOT_GROUP_PREVIEW_COUNT } from '../lib/scaleGrouping';
import { scaleKey } from '../lib/scaleCompatibility';
import { EmptyScalesIcon } from './icons/EmptyScalesIcon';
import { ScaleMatchEntry } from './ScaleMatchEntry';

export function ScaleCandidatesCell({
  scales,
  minCoveragePercent,
  activeTags,
  onToggleTag,
  onPullScale,
  onPullNewFile,
  pullStatus,
}: {
  scales: ScaleCandidate[];
  minCoveragePercent: number;
  activeTags: ActiveScaleTag[];
  onToggleTag: (root_name: string, scale_name: string) => void;
  onPullScale: (root_name: string, scale_name: string) => void;
  onPullNewFile: (root_name: string, scale_name: string) => void;
  pullStatus: Map<string, ApplyState>;
}) {
  const [expanded, setExpanded] = useState(false);

  // Once a filter is active, a row only earned its place in the table by
  // matching one of the active tags (see projectMatchesTags upstream) — so
  // once here, showing every other unrelated compatible scale is just noise;
  // narrow the row down to the scale(s) that actually match.
  const tagFilteredScales =
    activeTags.length > 0
      ? scales.filter((s) => activeTags.some((t) => candidateMatchesTag(s, t)))
      : scales;

  if (tagFilteredScales.length === 0) {
    return (
      <span className="scales-empty">
        <EmptyScalesIcon />
        No compatible scale found
      </span>
    );
  }

  const allGroups = groupByRootNote(tagFilteredScales);
  const visibleGroups = expanded ? allGroups : allGroups.slice(0, ROOT_GROUP_PREVIEW_COUNT);
  const hiddenGroups = expanded ? [] : allGroups.slice(ROOT_GROUP_PREVIEW_COUNT);
  const hiddenCount = hiddenGroups.reduce((sum, [, group]) => sum + group.length, 0);

  return (
    <div className="scales-by-root">
      {visibleGroups.map(([root, group]) => (
        <div key={root} className="scale-root-group">
          <span className="scale-root">{root}</span>
          <ul className="scales-list">
            {group.map((s) => (
              <ScaleMatchEntry
                key={s.scale_name}
                scale={s}
                minCoveragePercent={minCoveragePercent}
                activeTags={activeTags}
                pullState={pullStatus.get(scaleKey(s.root_name, s.scale_name))}
                onToggleTag={onToggleTag}
                onPullScale={onPullScale}
                onPullNewFile={onPullNewFile}
              />
            ))}
          </ul>
        </div>
      ))}
      {hiddenCount > 0 ? (
        <button type="button" className="scales-toggle" onClick={() => setExpanded(true)}>
          Show {hiddenCount} more
        </button>
      ) : (
        allGroups.length > ROOT_GROUP_PREVIEW_COUNT && (
          <button type="button" className="scales-toggle" onClick={() => setExpanded(false)}>
            Show fewer
          </button>
        )
      )}
    </div>
  );
}
