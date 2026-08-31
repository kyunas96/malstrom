import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AppliedScaleResult } from '../types/alsProject';
import type { ApplyState, PendingNewFilePull, PendingPull } from '../types/scaleMode';
import { scaleKey } from '../lib/scaleCompatibility';

export function usePullScale() {
  const [pullStatusByRow, setPullStatusByRow] = useState<Map<string, Map<string, ApplyState>>>(
    new Map(),
  );
  const [pendingPull, setPendingPull] = useState<PendingPull | null>(null);
  const [pendingNewFilePull, setPendingNewFilePull] = useState<PendingNewFilePull | null>(null);

  function setPullStatus(projectPath: string, key: string, status: ApplyState) {
    setPullStatusByRow((prev) => {
      const next = new Map(prev);
      const rowMap = new Map(next.get(projectPath));
      rowMap.set(key, status);
      next.set(projectPath, rowMap);
      return next;
    });
  }

  async function handlePullScale(
    projectPath: string,
    root_name: string,
    scale_name: string,
    overwrite: boolean,
    newFileName?: string,
  ) {
    const key = scaleKey(root_name, scale_name);
    setPullStatus(projectPath, key, { state: 'loading', message: '' });
    try {
      const result = await invoke<AppliedScaleResult>('apply_scale_to_project', {
        path: projectPath,
        rootName: root_name,
        scaleName: scale_name,
        overwrite,
        newFileName,
      });
      const totalApplied = result.clips_changed + result.clips_created + result.clips_corrected;
      const baseMessage =
        totalApplied > 0
          ? overwrite
            ? `Pulled into ${totalApplied} clip${totalApplied === 1 ? '' : 's'} → saved in place`
            : `Pulled into ${totalApplied} clip${totalApplied === 1 ? '' : 's'} → saved as ${result.new_path}`
          : 'No clips needed this scale — no file written';
      const versionWarning =
        result.schema_predates_clip_scale && result.clips_created > 0
          ? ' ⚠ This project predates Ableton’s per-clip Scale feature — open it in Ableton and save once (no changes needed), then Pull again, or the new scale may not show up.'
          : '';
      const skippedNote =
        result.clips_already_set > 0
          ? ` (${result.clips_already_set} clip${result.clips_already_set === 1 ? '' : 's'} already had a different valid scale and were left alone)`
          : '';
      setPullStatus(projectPath, key, {
        state: 'success',
        message: baseMessage + skippedNote + versionWarning,
      });
    } catch (err) {
      setPullStatus(projectPath, key, { state: 'error', message: String(err) });
    }
  }

  function requestPull(pending: PendingPull) {
    setPendingPull(pending);
  }

  function cancelPull() {
    setPendingPull(null);
  }

  function confirmPull() {
    if (!pendingPull) return;
    handlePullScale(pendingPull.projectPath, pendingPull.root_name, pendingPull.scale_name, true);
    setPendingPull(null);
  }

  function requestNewFilePull(pending: PendingNewFilePull) {
    setPendingNewFilePull(pending);
  }

  function cancelNewFilePull() {
    setPendingNewFilePull(null);
  }

  function confirmNewFilePull(fileName: string) {
    if (!pendingNewFilePull) return;
    handlePullScale(
      pendingNewFilePull.projectPath,
      pendingNewFilePull.root_name,
      pendingNewFilePull.scale_name,
      false,
      fileName,
    );
    setPendingNewFilePull(null);
  }

  return {
    pullStatusByRow,
    pendingPull,
    pendingNewFilePull,
    handlePullScale,
    requestPull,
    cancelPull,
    confirmPull,
    requestNewFilePull,
    cancelNewFilePull,
    confirmNewFilePull,
  };
}
