import { describe, expect, it } from 'vitest';
import { dummyAlsProjects } from '../fixtures/alsProjects';
import { matchingTagsForProject, projectMatchesTags, scaleKey } from './scaleCompatibility';
import type { ActiveScaleTag } from '../types/scaleFilter';

const sunsetDrive = dummyAlsProjects.find((p) => p.name === 'Sunset Drive')!;
const midnightLoop = dummyAlsProjects.find((p) => p.name === 'Midnight Loop')!;
const glassCorridor = dummyAlsProjects.find((p) => p.name === 'Glass Corridor')!;

function tag(root_name: string, scale_name: string, originPath: string): ActiveScaleTag {
  return { root_name, scale_name, originPath };
}

describe('scaleKey', () => {
  it('joins root and scale name', () => {
    expect(scaleKey('C', 'Major')).toBe('C::Major');
  });
});

describe('projectMatchesTags', () => {
  it('matches via a direct scale_name hit', () => {
    const tags = [tag('C', 'Major', sunsetDrive.path)];
    expect(projectMatchesTags(sunsetDrive, tags, 'any', 0)).toBe(true);
  });

  it('matches via an alternates entry, not just the primary label', () => {
    // Sunset Drive's "C Major" candidate lists "D Dorian" as an alternate.
    const tags = [tag('D', 'Dorian', midnightLoop.path)];
    expect(projectMatchesTags(sunsetDrive, tags, 'any', 0)).toBe(true);
  });

  it('excludes a candidate below minCoveragePercent from matching', () => {
    // "A Lydian" on Sunset Drive has coverage_percent 5.
    const tags = [tag('A', 'Lydian', sunsetDrive.path)];
    expect(projectMatchesTags(sunsetDrive, tags, 'any', 10)).toBe(false);
    expect(projectMatchesTags(sunsetDrive, tags, 'any', 0)).toBe(true);
  });

  it("mode 'all' requires every tag to hit", () => {
    const tags = [tag('C', 'Major', sunsetDrive.path), tag('D', 'Dorian', midnightLoop.path)];
    expect(projectMatchesTags(midnightLoop, tags, 'all', 0)).toBe(false);
    expect(projectMatchesTags(midnightLoop, tags, 'any', 0)).toBe(true);
  });

  it('empty tags array matches everything', () => {
    expect(projectMatchesTags(glassCorridor, [], 'any', 0)).toBe(true);
    expect(projectMatchesTags(glassCorridor, [], 'all', 0)).toBe(true);
  });
});

describe('matchingTagsForProject', () => {
  it('returns only the subset of tags a project matches', () => {
    const tags = [tag('C', 'Major', sunsetDrive.path), tag('B', 'Minor Pentatonic', glassCorridor.path)];
    expect(matchingTagsForProject(sunsetDrive, tags, 0)).toEqual([tags[0]]);
    expect(matchingTagsForProject(glassCorridor, tags, 0)).toEqual([tags[1]]);
  });
});
