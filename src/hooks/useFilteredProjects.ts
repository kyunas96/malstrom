import { useMemo } from 'react';
import type { AlsProject } from '../types/alsProject';
import type { ActiveScaleTag, TagMatchMode } from '../types/scaleFilter';
import { projectMatchesTags } from '../lib/scaleCompatibility';

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
  const scopedProjects = useMemo(() => {
    return projects.map((project) => ({
      ...project,
      scales: project.scales.filter(
        (s) => (showCommonScales && s.common) || (showExoticScales && !s.common),
      ),
    }));
  }, [projects, showCommonScales, showExoticScales]);

  const data = useMemo(() => {
    return scopedProjects.filter((p) =>
      projectMatchesTags(p, activeTags, tagMode, minCoveragePercent),
    );
  }, [scopedProjects, activeTags, tagMode, minCoveragePercent]);

  const nameFiltered = useMemo(() => {
    const needle = nameFilter.trim().toLowerCase();
    return needle ? data.filter((p) => p.name.toLowerCase().includes(needle)) : data;
  }, [data, nameFilter]);

  const { orderedData, dividerIndex } = useMemo(() => {
    if (pinnedPaths.length === 0) {
      return { orderedData: nameFiltered, dividerIndex: -1 };
    }
    const pinnedRows = pinnedPaths
      .map((path) => nameFiltered.find((p) => p.path === path))
      .filter((p): p is AlsProject => p !== undefined);
    const pinnedPathSet = new Set(pinnedRows.map((p) => p.path));
    const otherRows = nameFiltered.filter((p) => !pinnedPathSet.has(p.path));
    if (pinnedRows.length === 0 || otherRows.length === 0) {
      return { orderedData: [...pinnedRows, ...otherRows], dividerIndex: -1 };
    }
    return { orderedData: [...pinnedRows, ...otherRows], dividerIndex: pinnedRows.length };
  }, [nameFiltered, pinnedPaths]);

  return { orderedData, dividerIndex };
}
