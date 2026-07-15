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
  standardMaxUpgrade: number;
  somberMaxUpgrade: number;
  exactUpgrade: boolean;
  twoHanding: boolean;
  dlcScaling: boolean;
  scadutreeLevel: number;
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

export interface StatusBuildupDto {
  bleed: number;
  frost: number;
  poison: number;
  scarletRot: number;
  sleep: number;
  madness: number;
  death: number;
}

export interface AowEffectDto {
  effectId: number;
  effectName: string;
  role: string;
  activationTiming: string;
  isSupported: boolean;
  reason: string;
  attackPower: DamageBreakdownDto;
  statusBuildup: StatusBuildupDto;
}

export interface AowHitDto {
  sheetRow: number;
  hitOrder: number;
  rawName: string;
  damage: DamageBreakdownDto;
  statusBuildup: StatusBuildupDto;
  physicalAttackAttribute: string;
  buffActive: boolean;
  effects: AowEffectDto[];
  warnings: string[];
}

export interface AowActionDto {
  actionId: string;
  actionOrder: number;
  staminaCost: number;
  hits: AowHitDto[];
}

export interface AowRouteDto {
  routeId: string;
  routeLabel: string;
  routePriority: number;
  buffActivationActionId: string | null;
  actions: AowActionDto[];
  firstHitDamage: number;
  totalDamage: DamageBreakdownDto;
  totalStatusBuildup: StatusBuildupDto;
  totalStaminaCost: number;
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

export interface StartSearchResponseDto {
  jobId: string;
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
  aowRoute: AowRouteDto | null;
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
  dataManifest: DataManifestDto;
}

export interface DataManifestDto {
  schemaVersion: number;
  datasetVersion: string;
  modelVersion: string;
  id: string;
  label: string;
  appVersion: string;
  source: string;
  generatedAt: string;
  extractorVersion: string;
  provenance: string;
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

export interface BuildPresetV1 {
  version: 1;
  id: string;
  name: string;
  request: OptimizeRequestDto;
  selectedBuild: SolvedBuildDto | null;
  compareTarget: SolvedBuildDto | null;
  dataVersion: string;
  createdAt: string;
  updatedAt: string;
}

export interface SavedBuildIndexEntryV1 {
  id: string;
  name: string;
  dataVersion: string;
  updatedAt: string;
}

export interface SavedBuildIndexV1 {
  version: 1;
  builds: SavedBuildIndexEntryV1[];
}

export interface CompareControls {
  weaponTypeKey: OptionalText;
  weaponName: OptionalText;
  affinity: OptionalText;
  aowName: OptionalText;
  matchSelectedAow: boolean;
}
