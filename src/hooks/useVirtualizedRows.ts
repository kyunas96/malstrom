import { useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';

/**
 * Wraps @tanstack/react-virtual for a row list that may contain one extra
 * synthetic entry (the pinned/other-projects divider) not present in the
 * underlying `rowCount` -- `toRowIndex` maps a virtual index back to the
 * real row index, skipping over the divider slot.
 */
export function useVirtualizedRows({
  rowCount,
  dividerIndex,
}: {
  rowCount: number;
  dividerIndex: number;
}) {
  const tableContainerRef = useRef<HTMLDivElement>(null);

  // The divider row between pinned and other results is a synthetic entry
  // in the virtualized row list (not a table row), so index math for
  // measurement/sizing must account for it as rows shift.
  const virtualCount = rowCount + (dividerIndex >= 0 ? 1 : 0);

  const rowVirtualizer = useVirtualizer({
    count: virtualCount,
    getScrollElement: () => tableContainerRef.current,
    // Rows have variable height (grouped scale lists), so estimate and
    // remeasure each row's real height as it's rendered.
    estimateSize: () => 56,
    overscan: 8,
  });
  const virtualRows = rowVirtualizer.getVirtualItems();
  const totalSize = rowVirtualizer.getTotalSize();
  const paddingTop = virtualRows.length > 0 ? virtualRows[0].start : 0;
  const paddingBottom =
    virtualRows.length > 0 ? totalSize - virtualRows[virtualRows.length - 1].end : 0;

  function toRowIndex(virtualIndex: number): number {
    return dividerIndex >= 0 && virtualIndex > dividerIndex ? virtualIndex - 1 : virtualIndex;
  }

  return {
    tableContainerRef,
    rowVirtualizer,
    virtualRows,
    paddingTop,
    paddingBottom,
    toRowIndex,
  };
}
