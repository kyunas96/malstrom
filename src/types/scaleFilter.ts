export interface ActiveScaleTag {
  root_name: string;
  scale_name: string;
  /** Project path the tag was clicked from (AlsProject.path). */
  originPath: string;
}

export type TagMatchMode = 'any' | 'all';
