import { describe, expect, it } from 'vitest';
import { pinnedPathsFor, removeTag, toggleTag } from './useScaleTags';
import type { ActiveScaleTag } from '../types/scaleFilter';

function tag(root_name: string, scale_name: string, originPath: string): ActiveScaleTag {
  return { root_name, scale_name, originPath };
}

describe('toggleTag', () => {
  it('adds a tag not already present', () => {
    const result = toggleTag([], 'C', 'Major', '/a.als');
    expect(result).toEqual([tag('C', 'Major', '/a.als')]);
  });

  it('removes a tag that matches root, scale, and origin', () => {
    const existing = [tag('C', 'Major', '/a.als')];
    expect(toggleTag(existing, 'C', 'Major', '/a.als')).toEqual([]);
  });

  it('treats the same scale from a different origin as a distinct tag', () => {
    const existing = [tag('C', 'Major', '/a.als')];
    const result = toggleTag(existing, 'C', 'Major', '/b.als');
    expect(result).toEqual([tag('C', 'Major', '/a.als'), tag('C', 'Major', '/b.als')]);
  });
});

describe('removeTag', () => {
  it('removes only the matching tag', () => {
    const existing = [tag('C', 'Major', '/a.als'), tag('D', 'Dorian', '/a.als')];
    expect(removeTag(existing, 'C', 'Major', '/a.als')).toEqual([tag('D', 'Dorian', '/a.als')]);
  });
});

describe('pinnedPathsFor', () => {
  it('dedupes origin paths across multiple tags', () => {
    const tags = [tag('C', 'Major', '/a.als'), tag('D', 'Dorian', '/a.als'), tag('E', 'Phrygian', '/b.als')];
    expect(pinnedPathsFor(tags)).toEqual(['/a.als', '/b.als']);
  });

  it('returns an empty array with no tags', () => {
    expect(pinnedPathsFor([])).toEqual([]);
  });
});
