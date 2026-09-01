import { useMemo } from 'react';
import type { AlsProject } from '../types/alsProject';
import type { ActiveScaleTag, TagMatchMode } from '../types/scaleFilter';
import { projectMatchesTags } from '../lib/scaleCompatibility';

export function scopeToVisibleScales(
  projects: AlsProject[],
  showCommonScales: boolean,
  showExoticScales: boolean,
): AlsProject[] {
  return projects.map((project) => ({
    ...project,
    scales: project.scales.filter(
      (s) => (showCommonScales && s.common) || (showExoticScales && !s.common),
    ),
  }));
}

export function filterByName(projects: AlsProject[], nameFilter: string): AlsProject[] {
  const needle = nameFilter.trim().toLowerCase();
  return needle ? projects.filter((p) => p.name.toLowerCase().includes(needle)) : projects;
}

/** Moves pinned projects to the front, returning the boundary index between
 * pinned and unpinned rows (-1 when there's no split to show, i.e. nothing
 * pinned or nothing left unpinned). */
export function orderWithPinnedFirst(
  projects: AlsProject[],
  pinnedPaths: string[],
): { orderedData: AlsProject[]; dividerIndex: number } {
  if (pinnedPaths.length === 0) {
    return { orderedData: projects, dividerIndex: -1 };
  }
  const pinnedRows = pinnedPaths
    .map((path) => projects.find((p) => p.path === path))
    .filter((p): p is AlsProject => p !== undefined);
  const pinnedPathSet = new Set(pinnedRows.map((p) => p.path));
  const otherRows = projects.filter((p) => !pinnedPathSet.has(p.path));
  if (pinnedRows.length === 0 || otherRows.length === 0) {
    return { orderedData: [...pinnedRows, ...otherRows], dividerIndex: -1 };
  }
  return { orderedData: [...pinnedRows, ...otherRows], dividerIndex: pinnedRows.length };
}

export function useFilteredProjects({
  projects,
  showCommonScales,
  showExoticScales,
  activeTags,
  tagMode,
  minCoveragePercent,
  pinnedPaths,
  nameFilter,
}: {
  projects: AlsProject[];
  showCommonScales: boolean;
  showExoticScales: boolean;
  activeTags: ActiveScaleTag[];
  tagMode: TagMatchMode;
  minCoveragePercent: number;
  pinnedPaths: string[];
  nameFilter: string;
}) {
  const scopedProjects = useMemo(
    () => scopeToVisibleScales(projects, showCommonScales, showExoticScales),
    [projects, showCommonScales, showExoticScales],
  );

  const data = useMemo(() => {
    return scopedProjects.filter((p) =>
      projectMatchesTags(p, activeTags, tagMode, minCoveragePercent),
    );
  }, [scopedProjects, activeTags, tagMode, minCoveragePercent]);

  const nameFiltered = useMemo(() => filterByName(data, nameFilter), [data, nameFilter]);

  const { orderedData, dividerIndex } = useMemo(
    () => orderWithPinnedFirst(nameFiltered, pinnedPaths),
    [nameFiltered, pinnedPaths],
  );

  return { orderedData, dividerIndex };
}
