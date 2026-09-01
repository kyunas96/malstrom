import { describe, expect, it } from 'vitest';
import { filterByName, orderWithPinnedFirst, scopeToVisibleScales } from './useFilteredProjects';
import type { AlsProject, ScaleCandidate } from '../types/alsProject';

function scale(common: boolean): ScaleCandidate {
  return { root_name: 'C', scale_name: 'Major', common, score: 1, clip_count: 1, coverage_percent: 100, alternates: [] };
}

function project(path: string, name: string, scales: ScaleCandidate[] = []): AlsProject {
  return { path, name, scales };
}

describe('scopeToVisibleScales', () => {
  it('keeps only common scales when exotic is hidden', () => {
    const projects = [project('/a.als', 'A', [scale(true), scale(false)])];
    const result = scopeToVisibleScales(projects, true, false);
    expect(result[0].scales).toEqual([scale(true)]);
  });

  it('keeps only exotic scales when common is hidden', () => {
    const projects = [project('/a.als', 'A', [scale(true), scale(false)])];
    const result = scopeToVisibleScales(projects, false, true);
    expect(result[0].scales).toEqual([scale(false)]);
  });
});

describe('filterByName', () => {
  const projects = [project('/a.als', 'Sunset Drive'), project('/b.als', 'Midnight Loop')];

  it('returns everything when the filter is blank', () => {
    expect(filterByName(projects, '  ')).toEqual(projects);
  });

  it('matches case-insensitively against project name', () => {
    expect(filterByName(projects, 'sunset')).toEqual([projects[0]]);
  });
});

describe('orderWithPinnedFirst', () => {
  const a = project('/a.als', 'A');
  const b = project('/b.als', 'B');
  const c = project('/c.als', 'C');

  it('returns input unchanged with no divider when nothing is pinned', () => {
    expect(orderWithPinnedFirst([a, b, c], [])).toEqual({ orderedData: [a, b, c], dividerIndex: -1 });
  });

  it('moves pinned rows to the front and reports the divider index', () => {
    expect(orderWithPinnedFirst([a, b, c], ['/c.als'])).toEqual({
      orderedData: [c, a, b],
      dividerIndex: 1,
    });
  });

  it('omits the divider when every visible row is pinned', () => {
    expect(orderWithPinnedFirst([a, b], ['/a.als', '/b.als'])).toEqual({
      orderedData: [a, b],
      dividerIndex: -1,
    });
  });

  it('drops pinned paths that are no longer in the visible set', () => {
    expect(orderWithPinnedFirst([a], ['/a.als', '/missing.als'])).toEqual({
      orderedData: [a],
      dividerIndex: -1,
    });
  });
});
