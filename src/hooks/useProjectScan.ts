import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AlsProject } from '../types/alsProject';
import { dummyAlsProjects } from '../fixtures/alsProjects';

export function patchProjectScales(
  projects: AlsProject[],
  path: string,
  scales: AlsProject['scales'],
): AlsProject[] {
  return projects.map((p) => (p.path === path ? { ...p, scales } : p));
}

export function useProjectScan() {
  const [rootFolder, setRootFolder] = useState<string | null>(null);
  const [projects, setProjects] = useState<AlsProject[]>(dummyAlsProjects);
  const [loading, setLoading] = useState(false);
  const [progress, setProgress] = useState<{ completed: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleChooseFolder(onFolderChosen?: () => void) {
    setError(null);
    const selected = await open({
      directory: true,
      multiple: false,
    });

    if (typeof selected !== 'string') return;

    setRootFolder(selected);
    setLoading(true);
    setProgress(null);
    onFolderChosen?.();

    const unlisten = await listen<{ completed: number; total: number }>(
      'list-projects-progress',
      (event) => setProgress(event.payload),
    );

    try {
      const response = await invoke<AlsProject[]>('list_projects', {
        rootPath: selected,
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

  function updateProjectScales(path: string, scales: AlsProject['scales']) {
    setProjects((prev) => patchProjectScales(prev, path, scales));
  }

  return {
    rootFolder,
    projects,
    loading,
    progress,
    error,
    handleChooseFolder,
    updateProjectScales,
  };
}
