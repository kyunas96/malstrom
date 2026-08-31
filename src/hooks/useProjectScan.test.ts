import { describe, expect, it } from 'vitest';
import { patchProjectScales } from './useProjectScan';
import type { AlsProject, ScaleCandidate } from '../types/alsProject';

function project(path: string): AlsProject {
  return { path, name: path, scales: [] };
}

const newScales: ScaleCandidate[] = [
  {
    root_name: 'C',
    scale_name: 'Major',
    common: true,
    score: 10,
    clip_count: 2,
    coverage_percent: 100,
    alternates: [],
  },
];

describe('patchProjectScales', () => {
  it('replaces scales only on the matching project', () => {
    const projects = [project('/a.als'), project('/b.als')];
    const result = patchProjectScales(projects, '/b.als', newScales);

    expect(result[0].scales).toEqual([]);
    expect(result[1].scales).toBe(newScales);
    expect(result[1]).not.toBe(projects[1]);
  });

  it('is a no-op when the path is not in the list', () => {
    const projects = [project('/a.als')];
    expect(patchProjectScales(projects, '/missing.als', newScales)).toEqual(projects);
  });
});
