import { describe, expect, it } from 'vitest';
import { candidateMatchesTag, groupByRootNote } from './scaleGrouping';
import type { ScaleCandidate } from '../types/alsProject';

function candidate(
  root_name: string,
  scale_name: string,
  alternates: ScaleCandidate['alternates'] = [],
): ScaleCandidate {
  return { root_name, scale_name, common: true, score: 1, clip_count: 1, coverage_percent: 100, alternates };
}

describe('groupByRootNote', () => {
  it('groups candidates by root_name, preserving first-seen order', () => {
    const scales = [candidate('C', 'Major'), candidate('D', 'Dorian'), candidate('C', 'Minor')];
    expect(groupByRootNote(scales)).toEqual([
      ['C', [candidate('C', 'Major'), candidate('C', 'Minor')]],
      ['D', [candidate('D', 'Dorian')]],
    ]);
  });

  it('returns an empty array for no candidates', () => {
    expect(groupByRootNote([])).toEqual([]);
  });
});

describe('candidateMatchesTag', () => {
  it('matches when the tag hits the candidate directly', () => {
    const c = candidate('C', 'Major');
    expect(candidateMatchesTag(c, { root_name: 'C', scale_name: 'Major', originPath: '/a.als' })).toBe(
      true,
    );
  });

  it('matches when the tag hits one of the candidate alternates', () => {
    const c = candidate('C', 'Major', [{ root_name: 'D', scale_name: 'Dorian' }]);
    expect(candidateMatchesTag(c, { root_name: 'D', scale_name: 'Dorian', originPath: '/a.als' })).toBe(
      true,
    );
  });

  it('does not match an unrelated scale', () => {
    const c = candidate('C', 'Major');
    expect(candidateMatchesTag(c, { root_name: 'E', scale_name: 'Phrygian', originPath: '/a.als' })).toBe(
      false,
    );
  });
});
