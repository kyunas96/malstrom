// Mirrors src-tauri/src/xml.rs::ScaleCandidate and src-tauri/src/lib.rs::AlsProjectSummary,
// returned by the `list_projects` Tauri command.

export interface ScaleAlternate {
  root_name: string;
  scale_name: string;
}

export interface ScaleCandidate {
  root_name: string;
  scale_name: string;
  /** True for a common Western scale/mode, false for a less common "exotic" one. */
  common: boolean;
  score: number;
  clip_count: number;
  /** Percentage of the project's pitched clips this scale matches. */
  coverage_percent: number;
  /**
   * Other (root, scale) pairs that fit the exact same notes -- relative
   * modes of the same pitch collection tie exactly on pitch content alone,
   * so one is picked as the primary label and the rest are listed here.
   */
  alternates: ScaleAlternate[];
}

export interface AlsProject {
  path: string;
  name: string;
  /** Compatible scales, best match (highest score) first. */
  scales: ScaleCandidate[];
}

/** Result of the `apply_scale_to_project` command. */
export interface AppliedScaleResult {
  /** `null` when no clip needed changing, in which case no file was written. */
  new_path: string | null;
  clips_changed: number;
  clips_created: number;
  clips_corrected: number;
  clips_already_set: number;
  clips_incompatible: number;
  /**
   * True when the project predates Ableton's per-clip Scale feature (an old
   * file-format schema) -- any newly created ScaleInformation may not
   * actually be honored by Ableton until the project is resaved there once.
   */
  schema_predates_clip_scale: boolean;
  /** Fresh scale candidates for the written file, when one was written. */
  updated_scales: ScaleCandidate[] | null;
}
