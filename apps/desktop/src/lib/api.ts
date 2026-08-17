import { invoke } from "@tauri-apps/api/core";
import {
  AffinityWatchPayloadDto,
  AffinityWatchJobStatusDto,
  CatalogDto,
  DataManifestDto,
  OptimizeRequestDto,
  PathJobStatusDto,
  PathPreviewDto,
  SearchJobStatusDto,
  StartSearchResponseDto,
  ScalingDto,
  SolvedBuildDto,
  UpgradePointDto,
  WeaponProfileDto,
} from "./types";
import { upgradeCapForRow } from "./session";
import { STARTING_CLASS_METADATA } from "./session";

type MockWeapon = {
  weaponId: number;
  name: string;
  affinity: string;
  weaponTypeName: string;
  requirements: WeaponProfileDto["requirements"];
  scaling: ScalingDto;
  isSomber: boolean;
};

const MOCK_WEAPONS: MockWeapon[] = [
  mockWeapon(100, "Uchigatana", "Blood", "Katana", 11, 13, scaling(0.61, 0.93, 0, 0, 0.44)),
  mockWeapon(101, "Uchigatana", "Occult", "Katana", 11, 13, scaling(0.21, 0.33, 0, 0, 1.39)),
  mockWeapon(102, "Uchigatana", "Keen", "Katana", 11, 13, scaling(0.25, 1.2)),
  mockWeapon(200, "Zweihander", "Standard", "Colossal Sword", 19, 11, scaling(0.7, 0.35)),
  mockWeapon(300, "Ancient Meteoric Ore Greatsword", "Unique", "Great Katana", 35, 10, scaling(1, 0.2, 0, 0, 0.6), true),
];

const MOCK_AOW_NAMES = ["Bloodhound's Step", "Seppuku"];

const PREVIEW_SOLVED_BUILDS: SolvedBuildDto[] = [
  previewBuild(100, "Uchigatana", "Blood", 700, 84, 854, 2205, 700, { strStat: 13, dex: 22, intStat: 9, fai: 8, arc: 60 }),
  previewBuild(101, "Uchigatana", "Occult", 670, 72, 817, 2111, 670, { strStat: 13, dex: 22, intStat: 9, fai: 8, arc: 60 }),
  previewBuild(102, "Uchigatana", "Keen", 640, 45, 781, 2016, 640, { strStat: 13, dex: 60, intStat: 9, fai: 8, arc: 8 }),
  previewBuild(300, "Ancient Meteoric Ore Greatsword", "Unique", 590, 0, 708, 1815, 590, { strStat: 35, dex: 10, intStat: 9, fai: 8, arc: 20 }, {
    isSomber: true,
    upgrade: 10,
    aowId: null,
    aowName: null,
  }),
];

function mockWeapon(
  weaponId: number,
  name: string,
  affinity: string,
  weaponTypeName: string,
  reqStr: number,
  reqDex: number,
  weaponScaling: ScalingDto,
  isSomber = false,
): MockWeapon {
  return {
    weaponId,
    name,
    affinity,
    weaponTypeName,
    requirements: { strStat: reqStr, dex: reqDex, intStat: 0, fai: 0, arc: 0 },
    scaling: weaponScaling,
    isSomber,
  };
}

function scaling(str: number, dex: number, int = 0, fai = 0, arc = 0): ScalingDto {
  return { str, dex, int, fai, arc };
}

function previewBuild(
  weaponId: number,
  weaponName: string,
  affinity: string,
  physicalAr: number,
  bleedBuildup: number,
  aowFirstHitDamage: number,
  aowFullSequenceDamage: number,
  score: number,
  stats: SolvedBuildDto["stats"],
  options: Partial<Pick<SolvedBuildDto, "isSomber" | "upgrade" | "aowId" | "aowName">> = {},
): SolvedBuildDto {
  const weapon = MOCK_WEAPONS.find((row) => row.name === weaponName && row.affinity === affinity);
  if (!weapon) throw new Error(`Preview build has no weapon fixture: ${weaponName} / ${affinity}`);
  const isSomber = options.isSomber ?? weapon.isSomber;
  return {
    weaponId,
    weaponName,
    weaponTypeName: weapon.weaponTypeName,
    affinity,
    isSomber,
    upgrade: options.upgrade ?? (isSomber ? 10 : 25),
    stats,
    requirements: weapon.requirements,
    effectiveScaling: weapon.scaling,
    ar: { physical: physicalAr, magic: 0, fire: 0, lightning: 0, holy: 0, total: physicalAr },
    aowId: options.aowId ?? 1,
    aowName: options.aowName === undefined ? "Seppuku" : options.aowName,
    bleedBuildup,
    bleedBuildupAdd: 0,
    frostBuildup: 0,
    poisonBuildup: 0,
    scarletRotBuildup: 0,
    sleepBuildup: 0,
    madnessBuildup: 0,
    deathBuildup: 0,
    aowFirstHitDamage,
    aowFullSequenceDamage,
    aowRoute: null,
    score,
  };
}

export const hasTauriRuntime = () => "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (hasTauriRuntime()) {
    return invoke<T>(command, args);
  }
  const isDevPreview = Boolean((import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV);
  if (!isDevPreview) {
    throw new Error("Tauri runtime is required outside the explicit dev browser preview.");
  }
  return mockInvoke<T>(command, args);
}

export const api = {
  profiles: () => call<DataManifestDto[]>("get_profiles"),
  catalog: (profileId: string) => call<CatalogDto>("get_catalog", { profileId }),
  dataManifest: (profileId: string) => call<DataManifestDto>("get_data_manifest", { profileId }),
  weaponProfile: (profileId: string, weaponName: string, affinity: string | null) =>
    call<WeaponProfileDto>("get_weapon_profile", {
      request: { profileId, weaponName, affinity },
    }),
  startSearch: (request: OptimizeRequestDto) =>
    call<StartSearchResponseDto>("start_search", { request }),
  cancelSearch: (jobId: string) =>
    call<boolean>("cancel_search", { jobId }),
  searchStatus: (jobId: string) =>
    call<SearchJobStatusDto | null>("get_search_status", { jobId }),
  solveBuild: (
    base: OptimizeRequestDto,
    weaponName: string,
    affinity: string | null,
    aowName: string | null,
  ) =>
    call<SolvedBuildDto | null>("solve_build", {
      request: { base, weaponName, affinity, aowName },
    }),
  buildUpgradeSeries: (
    base: OptimizeRequestDto,
    solved: SolvedBuildDto,
    maxUpgrade: number,
  ) =>
    call<UpgradePointDto[]>("build_upgrade_series", {
      request: { base, solved, maxUpgrade },
    }),
  affinitiesForWeapon: (profileId: string, weaponName: string) =>
    call<string[]>("affinities_for_weapon", { profileId, weaponName }),
  compatibleAowNames: (profileId: string, weaponName: string | null, affinity: string | null) =>
    call<string[]>("compatible_aow_names", {
      request: { profileId, weaponName, affinity },
    }),
  compatibleAowNamesForAffinity: (profileId: string, affinity: string | null) =>
    call<string[]>("compatible_aow_names_for_affinity", {
      request: { profileId, affinity },
    }),
  weaponNamesForType: (profileId: string, weaponTypeKey: string | null) =>
    call<string[]>("weapon_names_for_type", {
      request: { profileId, weaponTypeKey },
    }),
  startPathPreview: (requests: Array<{
    base: OptimizeRequestDto;
    solved: SolvedBuildDto;
    levelsAhead: number;
    title: string;
  }>) => call<{ jobId: string }>("start_path_preview", { request: { requests } }),
  cancelPathPreview: (jobId: string) =>
    call<boolean>("cancel_path_preview", { jobId }),
  pathPreviewStatus: (jobId: string) =>
    call<PathJobStatusDto | null>("get_path_preview_status", { jobId }),
  startAffinityWatch: (
    base: OptimizeRequestDto,
    solved: SolvedBuildDto,
    levelsAhead: number,
  ) =>
    call<{ jobId: string }>("start_affinity_watch", {
      request: { base, solved, levelsAhead },
    }),
  cancelAffinityWatch: (jobId: string) =>
    call<boolean>("cancel_affinity_watch", { jobId }),
  affinityWatchStatus: (jobId: string) =>
    call<AffinityWatchJobStatusDto | null>("get_affinity_watch_status", { jobId }),
};

let mockJobSequence = 0;
let mockSearchJob: SearchJobStatusDto | null = null;
let mockPathJob: PathJobStatusDto | null = null;
let mockAffinityJob: AffinityWatchJobStatusDto | null = null;

function nextMockJobId(prefix: string): string {
  mockJobSequence += 1;
  return `browser-${prefix}-${mockJobSequence}`;
}

function mockJobMatches(
  status: { progress: { jobId: string } | null; finished: { jobId: string } | null } | null,
  jobId: string,
): boolean {
  return status?.progress?.jobId === jobId || status?.finished?.jobId === jobId;
}

async function mockStartSearch(args: Record<string, unknown> | undefined): Promise<StartSearchResponseDto> {
  const request = (args?.request as OptimizeRequestDto | undefined) ?? null;
  const jobId = nextMockJobId("search");
  const total = await mockSearchCombinationCount(request);
  mockSearchJob = {
    progress: { jobId, checked: 0, total, eligible: 0, bestScore: 0, elapsedMs: 0 },
    finished: null,
  };
  void mockRows(request).then((rows) => {
    if (!mockJobMatches(mockSearchJob, jobId) || mockSearchJob?.finished?.cancelled) return;
    mockSearchJob = {
      progress: {
        jobId,
        checked: total,
        total,
        eligible: rows.length,
        bestScore: rows[0]?.score ?? 0,
        elapsedMs: 0,
      },
      finished: { jobId, cancelled: false, rows, error: null },
    };
  }).catch((error) => {
    if (!mockJobMatches(mockSearchJob, jobId) || mockSearchJob?.finished?.cancelled) return;
    mockSearchJob = {
      progress: mockSearchJob?.progress ?? null,
      finished: { jobId, cancelled: false, rows: [], error: errorMessage(error) },
    };
  });
  return { jobId };
}

function mockSearchStatus(args: Record<string, unknown> | undefined): SearchJobStatusDto | null {
  const jobId = String(args?.jobId ?? "");
  return mockJobMatches(mockSearchJob, jobId) ? mockSearchJob : null;
}

function mockCancelSearch(args: Record<string, unknown> | undefined): boolean {
  const jobId = String(args?.jobId ?? "");
  const current = mockSearchJob;
  if (!current || !mockJobMatches(current, jobId) || current.finished) return false;
  mockSearchJob = {
    progress: current.progress,
    finished: { jobId, cancelled: true, rows: [], error: null },
  };
  return true;
}

async function mockStartPathPreview(args: Record<string, unknown> | undefined): Promise<StartSearchResponseDto> {
  const requests = ((args?.request as {
    requests?: Array<{ base: OptimizeRequestDto; solved: SolvedBuildDto; levelsAhead: number; title: string }>;
  } | undefined)?.requests) ?? [];
  const jobId = nextMockJobId("path");
  const total = Math.max(requests.reduce((sum, request) => sum + request.levelsAhead + 1, 0), 1);
  const first = requests[0];
  mockPathJob = {
    progress: {
      jobId,
      checked: 0,
      total,
      title: first?.title ?? "Selected",
      level: first?.base.characterLevel ?? 0,
    },
    finished: null,
  };
  void Promise.all(requests.map((request) => mockPathPreview({ request }))).then((paths) => {
    if (!mockJobMatches(mockPathJob, jobId) || mockPathJob?.finished?.cancelled) return;
    mockPathJob = {
      progress: { ...mockPathJob!.progress!, checked: total },
      finished: { jobId, cancelled: false, paths, error: null },
    };
  }).catch((error) => {
    if (!mockJobMatches(mockPathJob, jobId) || mockPathJob?.finished?.cancelled) return;
    mockPathJob = {
      progress: mockPathJob?.progress ?? null,
      finished: { jobId, cancelled: false, paths: [], error: errorMessage(error) },
    };
  });
  return { jobId };
}

function mockPathPreviewStatus(args: Record<string, unknown> | undefined): PathJobStatusDto | null {
  const jobId = String(args?.jobId ?? "");
  return mockJobMatches(mockPathJob, jobId) ? mockPathJob : null;
}

function mockCancelPathPreview(args: Record<string, unknown> | undefined): boolean {
  const jobId = String(args?.jobId ?? "");
  const current = mockPathJob;
  if (!current || !mockJobMatches(current, jobId) || current.finished) return false;
  mockPathJob = {
    progress: current.progress,
    finished: { jobId, cancelled: true, paths: [], error: null },
  };
  return true;
}

async function mockStartAffinityWatch(args: Record<string, unknown> | undefined): Promise<StartSearchResponseDto> {
  const request = args?.request as {
    base?: OptimizeRequestDto;
    solved?: SolvedBuildDto;
    levelsAhead?: number;
  } | undefined;
  const jobId = nextMockJobId("affinity");
  const total = Math.max((request?.levelsAhead ?? 0) + 1, 1);
  mockAffinityJob = {
    progress: {
      jobId,
      checked: 0,
      total,
      affinity: request?.solved?.affinity ?? "Standard",
      level: request?.base?.characterLevel ?? 0,
    },
    finished: null,
  };
  void mockAffinityWatch(args).then((payload) => {
    if (!mockJobMatches(mockAffinityJob, jobId) || mockAffinityJob?.finished?.cancelled) return;
    mockAffinityJob = {
      progress: { ...mockAffinityJob!.progress!, checked: total },
      finished: { jobId, cancelled: false, payload, error: null },
    };
  }).catch((error) => {
    if (!mockJobMatches(mockAffinityJob, jobId) || mockAffinityJob?.finished?.cancelled) return;
    mockAffinityJob = {
      progress: mockAffinityJob?.progress ?? null,
      finished: { jobId, cancelled: false, payload: null, error: errorMessage(error) },
    };
  });
  return { jobId };
}

function mockAffinityWatchStatus(args: Record<string, unknown> | undefined): AffinityWatchJobStatusDto | null {
  const jobId = String(args?.jobId ?? "");
  return mockJobMatches(mockAffinityJob, jobId) ? mockAffinityJob : null;
}

function mockCancelAffinityWatch(args: Record<string, unknown> | undefined): boolean {
  const jobId = String(args?.jobId ?? "");
  const current = mockAffinityJob;
  if (!current || !mockJobMatches(current, jobId) || current.finished) return false;
  mockAffinityJob = {
    progress: current.progress,
    finished: { jobId, cancelled: true, payload: null, error: null },
  };
  return true;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "get_profiles":
      return [mockDataManifest("vanilla"), mockDataManifest("convergence")] as T;
    case "get_catalog":
      return await mockCatalog(String(args?.profileId ?? "vanilla")) as T;
    case "get_data_manifest":
      return mockDataManifest(String(args?.profileId ?? "vanilla")) as T;
    case "get_weapon_profile":
      return await mockWeaponProfile(args) as T;
    case "start_search":
      return await mockStartSearch(args) as T;
    case "cancel_search":
      return mockCancelSearch(args) as T;
    case "get_search_status":
      return mockSearchStatus(args) as T;
    case "solve_build":
      return await mockSolveBuild(args) as T;
    case "build_upgrade_series":
      return await mockUpgradeSeries(args) as T;
    case "weapon_names_for_type": {
      const key = (args?.request as { weaponTypeKey?: string | null })?.weaponTypeKey;
      return await mockWeaponNamesForType(key ?? null) as T;
    }
    case "compatible_aow_names_for_affinity":
      return await mockCompatibleAowNamesForAffinity(args) as T;
    case "compatible_aow_names":
      return await mockCompatibleAowNames(args) as T;
    case "affinities_for_weapon":
      return await mockAffinitiesForWeapon(args) as T;
    case "start_path_preview":
      return await mockStartPathPreview(args) as T;
    case "cancel_path_preview":
      return mockCancelPathPreview(args) as T;
    case "get_path_preview_status":
      return mockPathPreviewStatus(args) as T;
    case "start_affinity_watch":
      return await mockStartAffinityWatch(args) as T;
    case "cancel_affinity_watch":
      return mockCancelAffinityWatch(args) as T;
    case "get_affinity_watch_status":
      return mockAffinityWatchStatus(args) as T;
    default:
      return null as T;
  }
}

async function mockCatalog(profileId = "vanilla"): Promise<CatalogDto> {
  const weaponTypeOptions = uniqueSorted(MOCK_WEAPONS.map((weapon) => weapon.weaponTypeName))
    .map((label) => ({ key: label, label }));
  return {
    weaponCount: MOCK_WEAPONS.length,
    aowCount: MOCK_AOW_NAMES.length,
    weaponNames: uniqueSorted(MOCK_WEAPONS.map((weapon) => weapon.name)),
    weaponTypeKeys: weaponTypeOptions.map((entry) => entry.label),
    classes: STARTING_CLASS_METADATA,
    weaponTypeOptions,
    aowNames: MOCK_AOW_NAMES,
    affinityNames: uniqueSorted(MOCK_WEAPONS.map((weapon) => weapon.affinity)),
    objectiveIds: profileId === "convergence"
      ? ["max_ar", "max_physical_ar", "max_ar_plus_bleed"]
      : ["max_ar", "max_physical_ar", "max_ar_plus_bleed", "aow_first_hit", "aow_full_sequence"],
    somberFilters: ["all", "standard_only", "somber_only"],
    dataManifest: mockDataManifest(profileId),
  };
}

function mockDataManifest(profileId = "vanilla"): DataManifestDto {
  const convergence = profileId === "convergence";
  return {
    schemaVersion: 3,
    datasetVersion: convergence ? "convergence-3.0.0.1" : "vanilla-1.16.1",
    modelVersion: "aow-routes-effects-v2-profile-rules",
    id: convergence ? "convergence-3.0.0.1" : "vanilla-1.16.1",
    label: convergence ? "Convergence 3.0.0.1" : "Vanilla 1.16.1",
    appVersion: "1.16.1",
    source: "ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx",
    generatedAt: "2026-05-18",
    extractorVersion: "phase1-python-v5-profile-rules",
    provenance: "mock snapshot",
    profile: {
      id: profileId,
      displayName: convergence ? "Convergence" : "Vanilla",
      gameVersion: "1.16.1",
      modVersion: convergence ? "3.0.0.1" : null,
    },
    capabilities: {
      weaponAr: true,
      statusBuildup: true,
      weaponPassives: true,
      aowCompatibility: true,
      aowDamage: !convergence,
      aowRoutes: !convergence,
    },
    rules: {
      standardMaxUpgrade: convergence ? 15 : 25,
      somberMaxUpgrade: convergence ? 15 : 10,
      separateUpgradeCaps: !convergence,
      scadutreeScaling: !convergence,
      zeroAttackElementUsesWeaponScaling: convergence,
      extendedScalingGrades: convergence,
      statusBuildupScales: !convergence,
    },
  };
}

async function mockWeaponProfile(args: Record<string, unknown> | undefined): Promise<WeaponProfileDto> {
  const request = args?.request as { weaponName: string; affinity: string | null };
  const matches = MOCK_WEAPONS.filter((row) =>
    row.name === request.weaponName && (!request.affinity || row.affinity === request.affinity),
  );
  const first = matches[0];
  const requirements = matches.reduce(
    (current, row) => ({
      strStat: Math.max(current.strStat, row.requirements.strStat),
      dex: Math.max(current.dex, row.requirements.dex),
      intStat: Math.max(current.intStat, row.requirements.intStat),
      fai: Math.max(current.fai, row.requirements.fai),
      arc: Math.max(current.arc, row.requirements.arc),
    }),
    { strStat: 0, dex: 0, intStat: 0, fai: 0, arc: 0 },
  );
  const maxUpgrade = first?.isSomber ? 10 : 25;
  return {
    requirements,
    maxUpgrade,
    isSomber: first?.isSomber ?? false,
    disablesTwoHandBonus: false,
    affinities: uniqueSorted(MOCK_WEAPONS.filter((row) => row.name === request.weaponName).map((row) => row.affinity)),
    compatibleAows: await mockCompatibleAowNames({ request }),
  };
}

async function mockWeaponNamesForType(weaponTypeKey: string | null): Promise<string[]> {
  return uniqueSorted(
    MOCK_WEAPONS
      .filter((row) => !weaponTypeKey || row.weaponTypeName === weaponTypeKey)
      .map((row) => row.name),
  );
}

async function mockAffinitiesForWeapon(args: Record<string, unknown> | undefined): Promise<string[]> {
  const weaponName = args?.weaponName as string | undefined;
  return uniqueSorted(MOCK_WEAPONS.filter((row) => row.name === weaponName).map((row) => row.affinity));
}

async function mockCompatibleAowNamesForAffinity(args: Record<string, unknown> | undefined): Promise<string[]> {
  void args;
  return MOCK_AOW_NAMES;
}

async function mockCompatibleAowNames(args: Record<string, unknown> | undefined): Promise<string[]> {
  const request = args?.request as { weaponName?: string | null; affinity?: string | null } | undefined;
  if (!request?.weaponName) return MOCK_AOW_NAMES;
  const weapon = MOCK_WEAPONS.find((row) =>
    row.name === request.weaponName && (!request.affinity || row.affinity === request.affinity)
  );
  return weapon?.isSomber ? [] : MOCK_AOW_NAMES;
}

async function mockSearchCombinationCount(request: OptimizeRequestDto | null): Promise<number> {
  const upgradeCount = PREVIEW_SOLVED_BUILDS.filter((row) => matchesPreviewRow(row, request)).reduce((total, row) => {
    if (!request) return total + 26;
    if (request.exactUpgrade) return total + 1;
    const cap = row.isSomber ? request.somberMaxUpgrade : request.standardMaxUpgrade;
    return total + Math.max(Number(cap) + 1, 1);
  }, 0);
  return Math.max(upgradeCount, 1);
}

async function mockRows(request: OptimizeRequestDto | null): Promise<SolvedBuildDto[]> {
  const topK = Math.min(Math.max(Number(request?.topK ?? 25), 1), 500);
  return PREVIEW_SOLVED_BUILDS.filter((row) => matchesPreviewRow(row, request)).slice(0, topK);
}

async function mockSolveBuild(args: Record<string, unknown> | undefined): Promise<SolvedBuildDto | null> {
  const request = args?.request as {
    base?: OptimizeRequestDto;
    weaponName?: string;
    affinity?: string | null;
    aowName?: string | null;
  } | undefined;
  if (!request?.weaponName) {
    return null;
  }
  const rows = await mockRows({
    ...request.base,
    weaponName: request.weaponName,
    affinity: request.affinity ?? request.base?.affinity ?? null,
    aowName: request.aowName ?? request.base?.aowName ?? null,
    topK: 1,
  } as OptimizeRequestDto);
  return rows[0] ?? null;
}

function matchesPreviewRow(row: SolvedBuildDto, request: OptimizeRequestDto | null): boolean {
  if (!request) return true;
  if (request.weaponName && row.weaponName !== request.weaponName) return false;
  if (request.affinity && row.affinity !== request.affinity) return false;
  if (request.aowName && row.aowName !== request.aowName) return false;
  if (request.weaponTypeKey && row.weaponTypeName !== request.weaponTypeKey) return false;
  if (request.somberFilter === "somber_only" && !row.isSomber) return false;
  if (request.somberFilter === "standard_only" && row.isSomber) return false;
  const cap = upgradeCapForRow(row, request);
  if (request.exactUpgrade && row.upgrade !== cap) return false;
  if (!request.exactUpgrade && row.upgrade > cap) return false;
  return true;
}

async function mockUpgradeSeries(args: Record<string, unknown> | undefined): Promise<UpgradePointDto[]> {
  const request = args?.request as {
    base?: OptimizeRequestDto;
    solved?: SolvedBuildDto;
    maxUpgrade?: number;
  } | undefined;
  const row = request?.solved;
  const maxUpgrade = Number(request?.maxUpgrade ?? 12);
  if (!row) {
    return [];
  }
  const objective = request?.base?.objective ?? "max_ar";
  return Array.from({ length: maxUpgrade + 1 }, (_, upgrade) => ({
    upgrade,
    metric: fixed1(metricPreview(row, objective) * Math.max(0.35, upgrade / Math.max(maxUpgrade, 1))),
  }));
}

function metricPreview(row: SolvedBuildDto, objective: OptimizeRequestDto["objective"]): number {
  switch (objective) {
    case "max_physical_ar":
      return row.ar.physical;
    case "aow_first_hit":
      return row.aowFirstHitDamage;
    case "aow_full_sequence":
      return row.aowFullSequenceDamage;
    case "max_ar_plus_bleed":
      return row.bleedBuildup;
    default:
      return row.ar.total;
  }
}

async function mockPathPreview(args: Record<string, unknown> | undefined): Promise<PathPreviewDto> {
  const request = args?.request as {
    base?: OptimizeRequestDto;
    solved?: SolvedBuildDto;
    levelsAhead?: number;
    title?: string;
  } | undefined;
  const base = request?.base;
  const solved = request?.solved;
  if (!base || !solved) {
    throw new Error("Path preview requires a base request and solved build.");
  }
  const levelsAhead = Math.max(0, Math.trunc(Number(request?.levelsAhead ?? 0)));
  const steps = [
    fixedPathStep(base.characterLevel, solved.stats, metricPreview(solved, base.objective), null),
  ];
  if (levelsAhead > 0) {
    steps.push(fixedPathStep(
      base.characterLevel + levelsAhead,
      { ...solved.stats, dex: Math.min(99, solved.stats.dex + 1) },
      metricPreview(solved, base.objective) + 10,
      "dex",
    ));
  }
  return { title: request?.title ?? "Path", solved, steps };
}

function fixedPathStep(
  level: number,
  stats: SolvedBuildDto["stats"],
  metric: number,
  addedStat: string | null,
): {
  level: number;
  stats: SolvedBuildDto["stats"];
  metric: number | null;
  score: number | null;
  addedStat: string | null;
  requirementGap: number;
} {
  return {
    level,
    stats,
    metric,
    score: metric,
    addedStat,
    requirementGap: 0,
  };
}

async function mockAffinityWatch(args: Record<string, unknown> | undefined): Promise<AffinityWatchPayloadDto> {
  const request = args?.request as {
    base?: OptimizeRequestDto;
    solved?: SolvedBuildDto;
    levelsAhead?: number;
  } | undefined;
  const base = request?.base;
  const solved = request?.solved;
  if (!base || !solved) {
    return { lines: [], breakpoints: [] };
  }
  const levels = Array.from(
    { length: Math.max(0, Math.trunc(Number(request.levelsAhead ?? 0))) + 1 },
    (_, index) => base.characterLevel + index,
  );
  const lines = PREVIEW_SOLVED_BUILDS.filter((row) => row.weaponName === solved.weaponName).map((row) => {
    const startMetric = metricPreview(row, base.objective);
    const points = levels.map((level, index) => ({
      level,
      metric: fixed1(startMetric + index * (row.affinity === solved.affinity ? 8 : 6)),
      solved: row,
    }));
    return {
      affinity: row.affinity,
      points,
      startMetric: points[0]?.metric ?? null,
      endMetric: points.at(-1)?.metric ?? null,
      finalBuild: row,
    };
  });
  return {
    lines,
    breakpoints: lines.length > 1 && levels.length > 1
      ? [{
        level: levels.at(-1) ?? base.characterLevel,
        outgoingAffinity: lines[0].affinity,
        incomingAffinity: lines[1].affinity,
        outgoingMetric: lines[0].endMetric,
        incomingMetric: lines[1].endMetric,
      }]
      : [],
  };
}

function fixed1(value: number): number {
  return Number(value.toFixed(1));
}

function uniqueSorted(values: string[]): string[] {
  return Array.from(new Set(values.filter(Boolean))).sort((left, right) => left.localeCompare(right));
}
