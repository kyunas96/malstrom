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
  };
}
