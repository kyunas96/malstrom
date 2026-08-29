import type { AlsProject } from './types/alsProject';
import type { ActiveScaleTag, TagMatchMode } from './types/scaleFilter';
import { colorForTagIndex } from './lib/tagColors';
import { projectScaleKeys, scaleKey } from './lib/scaleCompatibility';

export function ScaleFilterBar({
  activeTags,
  projects,
  mode,
  minCoveragePercent,
  onRemoveTag,
  onClearAll,
  onModeChange,
}: {
  activeTags: ActiveScaleTag[];
  projects: AlsProject[];
  mode: TagMatchMode;
  minCoveragePercent: number;
  onRemoveTag: (root_name: string, scale_name: string, originPath: string) => void;
  onClearAll: () => void;
  onModeChange: (mode: TagMatchMode) => void;
}) {
  if (activeTags.length === 0) {
    return (
      <div className="row scale-filter-bar is-empty" role="group" aria-label="Active scale filters">
        <span className="scale-filter-empty">No active filters — click a scale to filter</span>
      </div>
    );
  }

  return (
    <div className="row scale-filter-bar" role="group" aria-label="Active scale filters">
      {activeTags.map((tag, index) => {
        const project = projects.find((p) => p.path === tag.originPath);
        const color = colorForTagIndex(index);
        // A tag whose underlying candidate has dropped below the coverage
        // threshold stays listed but renders muted, per spec §7 — removing
        // it silently would be more surprising than an inert chip.
        const isInert =
          !project || !projectScaleKeys(project, minCoveragePercent).has(
            scaleKey(tag.root_name, tag.scale_name),
          );
        return (
          <span
            key={`${tag.root_name}::${tag.scale_name}::${tag.originPath}`}
            className={isInert ? 'scale-filter-chip scale-filter-chip-inert' : 'scale-filter-chip'}
            style={{ backgroundColor: color.fg, color: '#fff' }}
            title={isInert ? 'Below the current coverage threshold — not currently matching' : undefined}
          >
            {tag.scale_name} — from {project?.name ?? tag.originPath}
            <button
              type="button"
              className="scale-filter-chip-remove"
              aria-label={`Remove ${tag.scale_name} filter`}
              onClick={() => onRemoveTag(tag.root_name, tag.scale_name, tag.originPath)}
            >
              ×
            </button>
          </span>
        );
      })}
      {activeTags.length >= 2 && (
        <div className="scale-filter-mode" role="group" aria-label="Match mode">
          <button
            type="button"
            className={mode === 'any' ? 'filter-toggle active' : 'filter-toggle'}
            aria-pressed={mode === 'any'}
            onClick={() => onModeChange('any')}
          >
            Match: Any
          </button>
          <button
            type="button"
            className={mode === 'all' ? 'filter-toggle active' : 'filter-toggle'}
            aria-pressed={mode === 'all'}
            onClick={() => onModeChange('all')}
          >
            Match: All
          </button>
        </div>
      )}
      <button type="button" className="scale-filter-clear" onClick={onClearAll}>
        Clear all
      </button>
    </div>
  );
}
