import type { ScaleCandidate } from '../types/alsProject';
import type { ActiveScaleTag } from '../types/scaleFilter';
import type { ApplyState } from '../types/scaleMode';
import { candidateMatchesTag } from '../lib/scaleGrouping';
import { colorForTagIndex } from '../lib/tagColors';

export function ScaleMatchEntry({
  scale,
  minCoveragePercent,
  activeTags,
  pullState,
  onToggleTag,
  onPullScale,
  onPullNewFile,
}: {
  scale: ScaleCandidate;
  minCoveragePercent: number;
  activeTags: ActiveScaleTag[];
  pullState: ApplyState | undefined;
  onToggleTag: (root_name: string, scale_name: string) => void;
  onPullScale: (root_name: string, scale_name: string) => void;
  onPullNewFile: (root_name: string, scale_name: string) => void;
}) {
  const matchingIndexes = activeTags
    .map((t, i) => ({ t, i }))
    .filter(({ t }) => candidateMatchesTag(scale, t))
    .map(({ i }) => i);
  const isTagged = matchingIndexes.length > 0;
  const belowThreshold = scale.coverage_percent < minCoveragePercent;
  const style = isTagged
    ? { backgroundColor: colorForTagIndex(Math.min(...matchingIndexes)).fg }
    : undefined;
  const title =
    matchingIndexes.length > 1
      ? `Matches: ${matchingIndexes.map((i) => activeTags[i].scale_name).join(', ')}`
      : 'Click to tag/filter · hover for pull actions';
  const isPulling = pullState?.state === 'loading';

  return (
    <li>
      <div className="scale-match-wrap" title={title}>
        <button
          type="button"
          className={
            belowThreshold
              ? 'scale-match-value is-selectable scale-below-threshold'
              : `scale-match-value is-selectable${isTagged ? ' is-tagged' : ''}`
          }
          style={style}
          disabled={belowThreshold || isPulling}
          onClick={() => onToggleTag(scale.root_name, scale.scale_name)}
        >
          <span className="scale-match-header">
            <strong>{scale.scale_name}</strong>
            <span className="scale-match-percent">
              {isPulling ? 'Pulling…' : `${scale.coverage_percent}%`}
            </span>
          </span>
          <span className="scale-match-bar-track">
            <span
              className={belowThreshold ? 'scale-match-bar-fill is-weak' : 'scale-match-bar-fill'}
              style={{ width: `${scale.coverage_percent}%` }}
            />
          </span>
          <span className="scale-match-caption">
            {scale.clip_count} clip{scale.clip_count === 1 ? '' : 's'}
          </span>
        </button>
        {!belowThreshold && (
          <div className="scale-actions">
            <button
              type="button"
              className="scale-action-btn"
              title="Pull into a new file"
              aria-label="Pull into a new file"
              disabled={isPulling}
              onClick={(e) => {
                e.stopPropagation();
                onPullNewFile(scale.root_name, scale.scale_name);
              }}
            >
              📄
            </button>
            <button
              type="button"
              className="scale-action-btn"
              title="Pull and write in place"
              aria-label="Pull and write in place"
              disabled={isPulling}
              onClick={(e) => {
                e.stopPropagation();
                onPullScale(scale.root_name, scale.scale_name);
              }}
            >
              💾
            </button>
          </div>
        )}
      </div>
      {pullState && pullState.state !== 'loading' && (
        <span className="scale-pull-status">{pullState.message}</span>
      )}
    </li>
  );
}
