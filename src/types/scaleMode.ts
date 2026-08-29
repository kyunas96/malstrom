export type ApplyState = { state: 'loading' | 'success' | 'error'; message: string };

export type PendingPull = {
  projectPath: string;
  projectName: string;
  root_name: string;
  scale_name: string;
};

export type PendingNewFilePull = PendingPull & {
  defaultFileName: string;
};
