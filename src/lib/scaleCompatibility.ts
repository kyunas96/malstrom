import type { AlsProject } from '../types/alsProject';
import type { ActiveScaleTag, TagMatchMode } from '../types/scaleFilter';

export function scaleKey(root_name: string, scale_name: string): string {
  return `${root_name}::${scale_name}`;
}

export function projectScaleKeys(
  project: AlsProject,
  minCoveragePercent: number,
): Set<string> {
  const keys = new Set<string>();
  for (const candidate of project.scales) {
    if (candidate.coverage_percent < minCoveragePercent) continue;
    keys.add(scaleKey(candidate.root_name, candidate.scale_name));
    for (const alt of candidate.alternates) {
      keys.add(scaleKey(alt.root_name, alt.scale_name));
    }
  }
  return keys;
}

export function projectMatchesTags(
  project: AlsProject,
  tags: ActiveScaleTag[],
  mode: TagMatchMode,
  minCoveragePercent: number,
): boolean {
  if (tags.length === 0) return true;
  const keys = projectScaleKeys(project, minCoveragePercent);
  const tagKeys = tags.map((t) => scaleKey(t.root_name, t.scale_name));
  return mode === 'any' ? tagKeys.some((k) => keys.has(k)) : tagKeys.every((k) => keys.has(k));
}

export function matchingTagsForProject(
  project: AlsProject,
  tags: ActiveScaleTag[],
  minCoveragePercent: number,
): ActiveScaleTag[] {
  const keys = projectScaleKeys(project, minCoveragePercent);
  return tags.filter((t) => keys.has(scaleKey(t.root_name, t.scale_name)));
}
