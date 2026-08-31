import { describe, expect, it } from 'vitest';
import { upsertProject } from './useProjectScan';
import type { AlsProject, ScaleCandidate } from '../types/alsProject';

function project(path: string, scales: ScaleCandidate[] = []): AlsProject {
  return { path, name: path, scales };
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

describe('upsertProject', () => {
  it('replaces the matching project in place', () => {
    const projects = [project('/a.als'), project('/b.als')];
    const updated = project('/b.als', newScales);
    const result = upsertProject(projects, updated);

    expect(result).toHaveLength(2);
    expect(result[0]).toBe(projects[0]);
    expect(result[1]).toBe(updated);
  });

  it('appends a new project when its path is not in the list', () => {
    const projects = [project('/a.als')];
    const created = project('/b.als', newScales);
    const result = upsertProject(projects, created);

    expect(result).toEqual([projects[0], created]);
  });
});
