import { useMemo, useState } from 'react';
import type { ActiveScaleTag, TagMatchMode } from '../types/scaleFilter';

function isSameTag(a: ActiveScaleTag, root_name: string, scale_name: string, originPath: string) {
  return a.root_name === root_name && a.scale_name === scale_name && a.originPath === originPath;
}

export function toggleTag(
  tags: ActiveScaleTag[],
  root_name: string,
  scale_name: string,
  originPath: string,
): ActiveScaleTag[] {
  if (tags.some((t) => isSameTag(t, root_name, scale_name, originPath))) {
    return tags.filter((t) => !isSameTag(t, root_name, scale_name, originPath));
  }
  return [...tags, { root_name, scale_name, originPath }];
}

export function removeTag(
  tags: ActiveScaleTag[],
  root_name: string,
  scale_name: string,
  originPath: string,
): ActiveScaleTag[] {
  return tags.filter((t) => !isSameTag(t, root_name, scale_name, originPath));
}

export function pinnedPathsFor(tags: ActiveScaleTag[]): string[] {
  return [...new Set(tags.map((t) => t.originPath))];
}

export function useScaleTags() {
  const [activeTags, setActiveTags] = useState<ActiveScaleTag[]>([]);
  const [tagMode, setTagMode] = useState<TagMatchMode>('any');

  function handleToggleTag(root_name: string, scale_name: string, originPath: string) {
    setActiveTags((prev) => toggleTag(prev, root_name, scale_name, originPath));
  }

  function handleRemoveTag(root_name: string, scale_name: string, originPath: string) {
    setActiveTags((prev) => removeTag(prev, root_name, scale_name, originPath));
  }

  function clearTags() {
    setActiveTags([]);
  }

  const pinnedPaths = useMemo(() => pinnedPathsFor(activeTags), [activeTags]);

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
