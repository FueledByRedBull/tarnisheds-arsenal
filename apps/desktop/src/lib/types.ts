export type ObjectiveId =
  | "max_ar"
  | "max_physical_ar"
  | "max_ar_plus_bleed"
  | "aow_first_hit"
  | "aow_full_sequence";

export type WorkspaceTab = "rankings" | "compare" | "paths" | "affinity_watch";
export type OptionalText = string | null;

export interface CombatStateDto {
  strStat: number;
  dex: number;
  intStat: number;
  fai: number;
  arc: number;
}

export interface OptimizeRequestDto {
  className: string;
  characterLevel: number;
  vig: number;
  mnd: number;
  end: number;
  strStat: number;
  dex: number;
  intStat: number;
  fai: number;
  arc: number;
  minStr: number;
  minDex: number;
  minInt: number;
  minFai: number;
  minArc: number;
  lockStr: number | null;
  lockDex: number | null;
  lockInt: number | null;
  lockFai: number | null;
  lockArc: number | null;
  maxUpgrade: number;
  fixedUpgrade: number | null;
  twoHanding: boolean;
  weaponName: string | null;
  affinity: string | null;
  aowName: string | null;
  weaponTypeKey: string | null;
  somberFilter: string;
  objective: ObjectiveId;
  topK: number;
}

export interface DamageBreakdownDto {
  physical: number;
  magic: number;
  fire: number;
  lightning: number;
  holy: number;
  total: number;
}

export interface ScalingDto {
  str: number;
  dex: number;
  int: number;
  fai: number;
  arc: number;
}

export interface SearchEstimateDto {
  weaponCandidates: number;
  statCandidates: number;
  combinations: number;
}

export interface SolvedBuildDto {
  weaponId: number;
  weaponName: string;
  affinity: string;
  isSomber: boolean;
  upgrade: number;
  stats: CombatStateDto;
  ar: DamageBreakdownDto;
  aowId: number | null;
  aowName: string | null;
  bleedBuildup: number;
  bleedBuildupAdd: number;
  frostBuildup: number;
  poisonBuildup: number;
  scarletRotBuildup: number;
  aowFirstHitDamage: number;
  aowFullSequenceDamage: number;
  score: number;
}

export interface CatalogDto {
  weaponCount: number;
  aowCount: number;
  weaponNames: string[];
  weaponTypeKeys: string[];
  classes: ClassMetadataDto[];
  weaponTypeOptions: WeaponTypeOptionDto[];
  aowNames: string[];
  objectiveIds: ObjectiveId[];
  somberFilters: string[];
}

export interface EightStatsDto {
  vig: number;
  mnd: number;
  end: number;
  strStat: number;
  dex: number;
  intStat: number;
  fai: number;
  arc: number;
}

export interface ClassMetadataDto {
  name: string;
  baseLevel: number;
  baseTotal: number;
  baseStats: EightStatsDto;
}

export interface WeaponTypeOptionDto {
  key: string;
  label: string;
}

export interface WeaponProfileDto {
  requirements: CombatStateDto;
  maxUpgrade: number;
  isSomber: boolean;
  disablesTwoHandBonus: boolean;
  affinities: string[];
  compatibleAows: string[];
}

export interface UpgradePointDto {
  upgrade: number;
  metric: number;
}

export interface PathStepDto {
  level: number;
  stats: CombatStateDto;
  metric: number | null;
  score: number | null;
  addedStat: string | null;
  requirementGap: number;
}

export interface PathPreviewDto {
  title: string;
  solved: SolvedBuildDto;
  steps: PathStepDto[];
}

export interface AffinityWatchPointDto {
  level: number;
  metric: number | null;
  solved: SolvedBuildDto | null;
}

export interface AffinityWatchLineDto {
  affinity: string;
  points: AffinityWatchPointDto[];
  startMetric: number | null;
  endMetric: number | null;
  finalBuild: SolvedBuildDto | null;
}

export interface AffinityBreakpointDto {
  level: number;
  outgoingAffinity: string;
  incomingAffinity: string;
  outgoingMetric: number | null;
  incomingMetric: number | null;
}

export interface AffinityWatchPayloadDto {
  lines: AffinityWatchLineDto[];
  breakpoints: AffinityBreakpointDto[];
}

export interface SearchProgressDto {
  jobId: string;
  checked: number;
  total: number;
  eligible: number;
  bestScore: number;
  elapsedMs: number;
}

export interface SearchFinishedDto {
  jobId: string;
  cancelled: boolean;
  rows: SolvedBuildDto[];
  error: string | null;
}

export interface SearchJobStatusDto {
  progress: SearchProgressDto | null;
  finished: SearchFinishedDto | null;
}

export interface PathProgressDto {
  jobId: string;
  checked: number;
  total: number;
  title: string;
  level: number;
}

export interface PathFinishedDto {
  jobId: string;
  cancelled: boolean;
  paths: PathPreviewDto[];
  error: string | null;
}

export interface PathJobStatusDto {
  progress: PathProgressDto | null;
  finished: PathFinishedDto | null;
}

export interface AffinityWatchProgressDto {
  jobId: string;
  checked: number;
  total: number;
  affinity: string;
  level: number;
}

export interface AffinityWatchFinishedDto {
  jobId: string;
  cancelled: boolean;
  payload: AffinityWatchPayloadDto | null;
  error: string | null;
}

export interface AffinityWatchJobStatusDto {
  progress: AffinityWatchProgressDto | null;
  finished: AffinityWatchFinishedDto | null;
}

export interface Notice {
  scope: WorkspaceTab | "global";
  tone: "info" | "warning" | "error" | "success";
  message: string;
}

export interface CompareControls {
  weaponTypeKey: OptionalText;
  weaponName: OptionalText;
  affinity: OptionalText;
  aowName: OptionalText;
  matchSelectedAow: boolean;
}
