import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AlsProject } from '../types/alsProject';
import { dummyAlsProjects } from '../fixtures/alsProjects';

export function upsertProject(projects: AlsProject[], updated: AlsProject): AlsProject[] {
  const existing = projects.some((p) => p.path === updated.path);
  return existing
    ? projects.map((p) => (p.path === updated.path ? updated : p))
    : [...projects, updated];
}

export function useProjectScan() {
  const [rootFolder, setRootFolder] = useState<string | null>(null);
  const [projects, setProjects] = useState<AlsProject[]>(dummyAlsProjects);
  const [loading, setLoading] = useState(false);
  const [progress, setProgress] = useState<{ completed: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function scan(root: string) {
    setError(null);
    setRootFolder(root);
    setLoading(true);
    setProgress(null);

    const unlisten = await listen<{ completed: number; total: number }>(
      'list-projects-progress',
      (event) => setProgress(event.payload),
    );

    try {
      const response = await invoke<AlsProject[]>('list_projects', {
        rootPath: root,
      });
      setProjects(response);
    } catch (err) {
      setError(String(err));
    } finally {
      unlisten();
      setLoading(false);
      setProgress(null);
    }
  }

  async function handleChooseFolder(onFolderChosen?: () => void) {
    const selected = await open({
      directory: true,
      multiple: false,
    });

    if (typeof selected !== 'string') return;
    onFolderChosen?.();
    await scan(selected);
  }

  /** Re-scans the current root folder, e.g. after Mashup writes a new file
   * into it -- picks the file up without asking the user to re-choose the
   * folder they're already pointed at. */
  function rescan() {
    if (rootFolder) return scan(rootFolder);
  }

  function upsertProjectFromResult(updated: AlsProject) {
    setProjects((prev) => upsertProject(prev, updated));
  }

  return {
    rootFolder,
    projects,
    loading,
    progress,
    error,
    handleChooseFolder,
    upsertProjectFromResult,
    rescan,
  };
}
