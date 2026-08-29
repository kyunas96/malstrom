import { useMemo, useState } from 'react';
import type { ActiveScaleTag, TagMatchMode } from '../types/scaleFilter';

function isSameTag(a: ActiveScaleTag, root_name: string, scale_name: string, originPath: string) {
  return a.root_name === root_name && a.scale_name === scale_name && a.originPath === originPath;
}

export function useScaleTags() {
  const [activeTags, setActiveTags] = useState<ActiveScaleTag[]>([]);
  const [tagMode, setTagMode] = useState<TagMatchMode>('any');

  function handleToggleTag(root_name: string, scale_name: string, originPath: string) {
    setActiveTags((prev) => {
      if (prev.some((t) => isSameTag(t, root_name, scale_name, originPath))) {
        return prev.filter((t) => !isSameTag(t, root_name, scale_name, originPath));
      }
      return [...prev, { root_name, scale_name, originPath }];
    });
  }

  function handleRemoveTag(root_name: string, scale_name: string, originPath: string) {
    setActiveTags((prev) => prev.filter((t) => !isSameTag(t, root_name, scale_name, originPath)));
  }

  function clearTags() {
    setActiveTags([]);
  }

  const pinnedPaths = useMemo(
    () => [...new Set(activeTags.map((t) => t.originPath))],
    [activeTags],
  );

  return {
    activeTags,
    tagMode,
    setTagMode,
    pinnedPaths,
    handleToggleTag,
    handleRemoveTag,
    clearTags,
  };
}
