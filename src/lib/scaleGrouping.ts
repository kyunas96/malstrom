// Naming convention: root_name/scale_name stay snake_case throughout the UI
// layer because they mirror the Rust/Tauri backend's field names; all other
// state (minCoveragePercent, activeTags, ...) stays camelCase.
import type { ScaleCandidate } from '../types/alsProject';
import type { ActiveScaleTag } from '../types/scaleFilter';
import { scaleKey } from './scaleCompatibility';

// Backends returns candidates sorted by coverage (strongest match first);
// only show the strongest few by default since the tail is mostly weak,
// partial matches that add noise for a non-expert reader. Counted in root
// groups (the grid's columns), not flat scale entries, since the grid is
// 4 columns wide and truncation should land on a clean row boundary.
export const ROOT_GROUP_PREVIEW_COUNT = 8;

export function groupByRootNote(scales: ScaleCandidate[]): [string, ScaleCandidate[]][] {
  const groups = new Map<string, ScaleCandidate[]>();
  for (const s of scales) {
    const group = groups.get(s.root_name);
    if (group) {
      group.push(s);
    } else {
      groups.set(s.root_name, [s]);
    }
  }
  return [...groups.entries()];
}

/**
 * A candidate matches a tag either directly, or via one of its alternates
 * (relative modes that tie on pitch content -- see ScaleCandidate.alternates).
 * Shared by row-level filtering and per-entry tag-color highlighting so the
 * two can't drift apart on what counts as "matches this tag".
 */
export function candidateMatchesTag(candidate: ScaleCandidate, tag: ActiveScaleTag): boolean {
  const tagKey = scaleKey(tag.root_name, tag.scale_name);
  if (scaleKey(candidate.root_name, candidate.scale_name) === tagKey) return true;
  return candidate.alternates.some((alt) => scaleKey(alt.root_name, alt.scale_name) === tagKey);
}
