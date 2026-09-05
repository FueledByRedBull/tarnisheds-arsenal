export type ObjectiveId =
  | "max_ar"
  | "max_physical_ar"
  | "max_ar_plus_bleed"
  | "aow_first_hit"
  | "aow_full_sequence";

export type WorkspaceTab = "rankings" | "compare" | "paths" | "affinity_watch";
export type OptionalText = string | null;
export type ResultGroupingId = "automatic" | "weapon" | "loadout";
export type FilterMode = "include" | "exclude";
export type FilterDimensionId =
  | "weapon_family"
  | "weapon_type"
  | "affinity"
  | "aow"
  | "reinforcement"
  | "coverage";

export interface StableFilterEntryDto {
  dimension: FilterDimensionId;
  id: string;
  mode: FilterMode;
}

export interface StableFilterSetDto {
  version: 1;
  entries: StableFilterEntryDto[];
}

export interface CombatStateDto {
  strStat: number;
  dex: number;
  intStat: number;
  fai: number;
  arc: number;
}

export interface OptimizeRequestDto {
  profileId: string;
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
  filters: StableFilterSetDto;
  resultGrouping: ResultGroupingId;
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
  poiseDamage: number;
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
  totalPoiseDamage: number;
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

export interface StartSearchResponseDto {
  jobId: string;
}

export interface SolvedBuildDto {
  weaponId: number;
  weaponName: string;
  weaponTypeName?: string;
  affinity: string;
  isSomber: boolean;
  upgrade: number;
  stats: CombatStateDto;
  requirements?: CombatStateDto;
  effectiveScaling?: ScalingDto;
  ar: DamageBreakdownDto;
  aowId: number | null;
  aowName: string | null;
  bleedBuildup: number;
  bleedBuildupAdd: number;
  frostBuildup: number;
  poisonBuildup: number;
  scarletRotBuildup: number;
  sleepBuildup: number;
  madnessBuildup: number;
  deathBuildup: number;
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
  affinityNames: string[];
  objectiveIds: ObjectiveId[];
  somberFilters: string[];
  filterDimensions: FilterDimensionDto[];
  dataManifest: DataManifestDto;
}

export interface FilterDimensionDto {
  id: FilterDimensionId;
  label: string;
  options: FilterOptionDto[];
}

export interface FilterOptionDto {
  id: string;
  label: string;
  count: number;
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
  profile: ProfileMetadataDto;
  capabilities: ProfileCapabilitiesDto;
  rules: ProfileRulesDto;
}

export interface ProfileMetadataDto {
  id: string;
  displayName: string;
  gameVersion: string;
  modVersion: string | null;
}

export interface ProfileCapabilitiesDto {
  classBudget: boolean;
  weaponAr: boolean;
  weaponArForAmmunition: boolean;
  statusBuildup: boolean;
  weaponPassives: boolean;
  aowCompatibility: boolean;
  aowDamage: boolean;
  aowRoutes: boolean;
}

export interface ProfileRulesDto {
  standardMaxUpgrade: number;
  somberMaxUpgrade: number;
  separateUpgradeCaps: boolean;
  scadutreeScaling: boolean;
  zeroAttackElementUsesWeaponScaling: boolean;
  extendedScalingGrades: boolean;
  statusBuildupScales: boolean;
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
  canChangeAow: boolean;
  nativeSkillName: string | null;
  requirements: CombatStateDto;
  maxUpgrade: number;
  isSomber: boolean;
  disablesTwoHandBonus: boolean;
  forcesTwoHanding: boolean;
  weight: number;
  moveCount: number;
  oneHandedPoise: DisplayPoiseDamageDto;
  twoHandedPoise: DisplayPoiseDamageDto;
  affinities: string[];
  compatibleAows: string[];
}

export interface DisplayPoiseDamageDto {
  light: string;
  heavy: string;
  chargedHeavy: string;
  jumpingLight: string;
  jumpingHeavy: string;
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

export type PathModeId = "no_respec" | "optimum_envelope";

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
  lead: number | null;
  leadPercent: number | null;
  quality: "tie" | "narrow" | "clear" | "unknown";
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
  profileId: string;
  request: OptimizeRequestDto;
  selectedBuild: SolvedBuildDto | null;
  compareTarget: SolvedBuildDto | null;
  dataVersion: string;
  createdAt: string;
  updatedAt: string;
}

export interface BuildPresetV2 {
  version: 2;
  id: string;
  name: string;
  profileId: string;
  request: OptimizeRequestDto;
  selectedBuild: SolvedBuildDto | null;
  compareTarget: SolvedBuildDto | null;
  compareBench: SolvedBuildDto[];
  dataVersion: string;
  createdAt: string;
  updatedAt: string;
}

export type BuildPreset = BuildPresetV2;

export interface SavedBuildIndexEntryV1 {
  id: string;
  name: string;
  profileId: string;
  dataVersion: string;
  updatedAt: string;
}

export interface SavedBuildIndexV1 {
  version: 1;
  builds: SavedBuildIndexEntryV1[];
}

export interface CompareControls {
  filters: StableFilterSetDto;
  weaponName: OptionalText;
  aowName: OptionalText;
  matchSelectedAow: boolean;
  includeSmithing: boolean;
  includeSomber: boolean;
}
