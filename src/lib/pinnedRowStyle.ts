import { colorForTagIndex } from './tagColors';
import type { ActiveScaleTag } from '../types/scaleFilter';
import type { CSSProperties } from 'react';

// Returns a CSS custom property rather than a literal border-left so
// `.scales-card-row.is-pinned` (in App.css) owns the border's structural
// properties (width/style) and only the per-tag color is set inline.
export function pinnedRowAccentStyle(projectPath: string, activeTags: ActiveScaleTag[]): CSSProperties {
  const tagIndex = activeTags.findIndex((t) => t.originPath === projectPath);
  const color = colorForTagIndex(Math.max(tagIndex, 0));
  return { '--pinned-row-accent': color.fg } as CSSProperties;
}
