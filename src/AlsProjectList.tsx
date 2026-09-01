import { useMemo, useState } from 'react';
import { createColumnHelper, flexRender, getCoreRowModel, useReactTable } from '@tanstack/react-table';
import type { AlsProject } from './types/alsProject';
import { pinnedRowAccentStyle } from './lib/pinnedRowStyle';
import { ScaleFilterBar } from './ScaleFilterBar';
import { ProjectNameCell } from './components/ProjectNameCell';
import { ScaleCandidatesCell } from './components/ScaleCandidatesCell';
import { ScaleFilterControls } from './components/ScaleFilterControls';
import { PullConfirmDialog } from './components/PullConfirmDialog';
import { PullNewFileDialog } from './components/PullNewFileDialog';
import { useProjectScan } from './hooks/useProjectScan';
import { useScaleTags } from './hooks/useScaleTags';
import { useFilteredProjects } from './hooks/useFilteredProjects';
import { usePullScale } from './hooks/usePullScale';
import { useVirtualizedRows } from './hooks/useVirtualizedRows';
import { useLiveDbPath } from './hooks/useLiveDbPath';
import { deriveDefaultFileName } from './lib/fileName';

const columnHelper = createColumnHelper<AlsProject>();

export function AlsProjectList() {
  const {
    rootFolder,
    projects,
    loading,
    progress,
    error,
    handleChooseFolder,
    upsertProjectFromResult,
  } = useProjectScan();
  const { liveDbPath, chooseLiveDbFile } = useLiveDbPath();
  const [showCommonScales, setShowCommonScales] = useState(true);
  const [showExoticScales, setShowExoticScales] = useState(false);
  const [minCoveragePercent, setMinCoveragePercent] = useState(0);
  const [nameFilter, setNameFilter] = useState('');

  const { activeTags, tagMode, setTagMode, pinnedPaths, handleToggleTag, handleRemoveTag, clearTags } =
    useScaleTags();

  const { orderedData, dividerIndex } = useFilteredProjects({
    projects,
    showCommonScales,
    showExoticScales,
    activeTags,
    tagMode,
    minCoveragePercent,
    pinnedPaths,
    nameFilter,
  });

  const {
    pullStatusByRow,
    pendingPull,
    pendingNewFilePull,
    requestPull,
    cancelPull,
    confirmPull,
    requestNewFilePull,
    cancelNewFilePull,
    confirmNewFilePull,
  } = usePullScale(upsertProjectFromResult);

  const columns = useMemo(
    () => [
      columnHelper.accessor('name', {
        header: 'Project',
        cell: (info) => (
          <ProjectNameCell
            name={info.getValue()}
            path={info.row.original.path}
            pinnedPaths={pinnedPaths}
            activeTags={activeTags}
            liveDbPath={liveDbPath}
          />
        ),
      }),
      columnHelper.accessor('scales', {
        header: 'Compatible Scales',
        cell: (info) => (
          <ScaleCandidatesCell
            scales={info.getValue()}
            minCoveragePercent={minCoveragePercent}
            activeTags={activeTags}
            onToggleTag={(root_name, scale_name) =>
              handleToggleTag(root_name, scale_name, info.row.original.path)
            }
            onPullScale={(root_name, scale_name) =>
              requestPull({
                projectPath: info.row.original.path,
                projectName: info.row.original.name,
                root_name,
                scale_name,
              })
            }
            onPullNewFile={(root_name, scale_name) =>
              requestNewFilePull({
                projectPath: info.row.original.path,
                projectName: info.row.original.name,
                root_name,
                scale_name,
                defaultFileName: deriveDefaultFileName(info.row.original.name, root_name, scale_name),
              })
            }
            pullStatus={pullStatusByRow.get(info.row.original.path) ?? new Map()}
          />
        ),
      }),
    ],
    [pinnedPaths, activeTags, minCoveragePercent, pullStatusByRow, requestPull, requestNewFilePull],
  );

  const table = useReactTable({
    data: orderedData,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  const rows = table.getRowModel().rows;
  const { tableContainerRef, rowVirtualizer, virtualRows, paddingTop, paddingBottom, toRowIndex } =
    useVirtualizedRows({ rowCount: rows.length, dividerIndex });

  const chooseFolder = () =>
    handleChooseFolder(() => {
      clearTags();
      setTagMode('any');
    });

  if (!rootFolder || loading) {
    return (
      <section className="folder-gate">
        <button type="button" onClick={chooseFolder} disabled={loading}>
          Choose Root Folder
        </button>
        {rootFolder && <p>{rootFolder}</p>}
        {loading && (
          <>
            <progress value={progress?.completed} max={progress?.total} />
            <p>
              {progress ? `Scanning folder… (${progress.completed}/${progress.total})` : 'Scanning folder…'}
            </p>
          </>
        )}
        {error && <p className="error-text">{error}</p>}
      </section>
    );
  }

  return (
    <section className="results-section">
      <div className="folder-bar">
        <p className="folder-bar-path" title={rootFolder}>
          {rootFolder}
        </p>
        <button type="button" className="folder-bar-change" onClick={chooseFolder}>
          Change Folder
        </button>
        <button type="button" className="folder-bar-change" onClick={chooseLiveDbFile} title={liveDbPath ?? undefined}>
          {liveDbPath ? 'Live Database: Set' : 'Set Live Database…'}
        </button>
      </div>
      {error && <p className="error-text">{error}</p>}
      <div className="scales-header">
        <div className="scales-header-title">
          <h2>Compatible Scales</h2>
          <p className="scale-mode-hint">
            Click a scale to tag it and filter for other compatible projects — hover it to pull the
            scale into this project as a new file or in place.
          </p>
        </div>
        <div className="scales-header-controls">
          <input
            type="text"
            className="name-filter-input"
            placeholder="Filter by name…"
            value={nameFilter}
            onChange={(e) => setNameFilter(e.target.value)}
          />
          <ScaleFilterBar
            activeTags={activeTags}
            projects={projects}
            mode={tagMode}
            minCoveragePercent={minCoveragePercent}
            onRemoveTag={handleRemoveTag}
            onClearAll={clearTags}
            onModeChange={setTagMode}
          />
          <ScaleFilterControls
            showCommonScales={showCommonScales}
            showExoticScales={showExoticScales}
            minCoveragePercent={minCoveragePercent}
            onToggleCommon={() => setShowCommonScales((prev) => !prev)}
            onToggleExotic={() => setShowExoticScales((prev) => !prev)}
            onMinCoverageChange={setMinCoveragePercent}
          />
        </div>
      </div>
      <div className="als-project-table-scroll" ref={tableContainerRef}>
        <div className="scales-card">
          <div className="scales-card-header">
            <span>Project</span>
            <span>Compatible Scales</span>
          </div>
          {paddingTop > 0 && <div style={{ height: paddingTop }} />}
          {virtualRows.map((virtualRow) => {
            if (dividerIndex >= 0 && virtualRow.index === dividerIndex) {
              return (
                <div
                  key="scale-filter-divider"
                  ref={rowVirtualizer.measureElement}
                  data-index={virtualRow.index}
                  className="scales-divider-row"
                >
                  Other compatible projects
                </div>
              );
            }
            const row = rows[toRowIndex(virtualRow.index)];
            const isPinned = pinnedPaths.includes(row.original.path);
            return (
              <div
                key={row.id}
                ref={rowVirtualizer.measureElement}
                data-index={virtualRow.index}
                className={isPinned ? 'scales-card-row is-pinned' : 'scales-card-row'}
                style={isPinned ? pinnedRowAccentStyle(row.original.path, activeTags) : undefined}
              >
                {row.getVisibleCells().map((cell) => (
                  <div key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</div>
                ))}
              </div>
            );
          })}
          {paddingBottom > 0 && <div style={{ height: paddingBottom }} />}
        </div>
      </div>
      {pendingPull && (
        <PullConfirmDialog pending={pendingPull} onCancel={cancelPull} onConfirm={confirmPull} />
      )}
      {pendingNewFilePull && (
        <PullNewFileDialog
          pending={pendingNewFilePull}
          onCancel={cancelNewFilePull}
          onConfirm={confirmNewFilePull}
        />
      )}
    </section>
  );
}
