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
  SearchEstimateDto,
  ScalingDto,
  SolvedBuildDto,
  UpgradePointDto,
  WeaponProfileDto,
} from "./types";
import { upgradeCapForRow } from "./session";
import { STARTING_CLASS_METADATA } from "./session";

type CsvRow = Record<string, string>;

const FIXTURE_WEAPONS: CsvRow[] = [
  fixtureWeapon("100", "Uchigatana", "Blood", "Katana", "Katana", "11", "13", "0.61", "0.93", "0", "0", "0.44", "700"),
  fixtureWeapon("101", "Uchigatana", "Occult", "Katana", "Katana", "11", "13", "0.21", "0.33", "0", "0", "1.39", "670"),
  fixtureWeapon("102", "Uchigatana", "Keen", "Katana", "Katana", "11", "13", "0.25", "1.2", "0", "0", "0", "640"),
  fixtureWeapon("200", "Zweihander", "Standard", "Colossal Sword", "Colossal Sword", "19", "11", "0.7", "0.35", "0", "0", "0", "610"),
  fixtureWeapon("300", "Ancient Meteoric Ore Greatsword", "Unique", "Great Katana", "Great Katana", "35", "10", "1.0", "0.2", "0", "0", "0.6", "590", "1"),
];

const FIXTURE_AOWS: CsvRow[] = [
  { aow_id: "1", name: "Seppuku" },
  { aow_id: "2", name: "Bloodhound's Step" },
];

const FIXTURE_AOW_AFFINITIES: CsvRow[] = [
  "Blood",
  "Occult",
  "Keen",
  "Standard",
].flatMap((affinity) => FIXTURE_AOWS.map((aow) => ({ affinity, name: aow.name, aow_id: aow.aow_id })));

const FIXTURE_AOW_WEAPON_COMPAT: CsvRow[] = FIXTURE_WEAPONS
  .filter((weapon) => weapon.name !== "Ancient Meteoric Ore Greatsword")
  .flatMap((weapon) => FIXTURE_AOWS.map((aow) => ({
    weapon_id: weapon.weapon_id,
    weapon_name: weapon.name,
    affinity: weapon.affinity,
    aow_id: aow.aow_id,
    aow_name: aow.name,
  })));

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

function fixtureWeapon(
  weaponId: string,
  name: string,
  affinity: string,
  weaponTypeName: string,
  weaponTypeKeys: string,
  reqStr: string,
  reqDex: string,
  strScaling: string,
  dexScaling: string,
  intScaling: string,
  faiScaling: string,
  arcScaling: string,
  previewTotalAr: string,
  isSomber = "0",
): CsvRow {
  return {
    weapon_id: weaponId,
    name,
    affinity,
    weapon_type_name: weaponTypeName,
    weapon_type_keys: weaponTypeKeys,
    req_str: reqStr,
    req_dex: reqDex,
    req_int: "0",
    req_fai: "0",
    req_arc: "0",
    disable_two_hand_bonus: "0",
    is_somber: isSomber,
    reinforce_type: "0",
    native_skill_id: "",
    native_skill_name: "",
    str_scaling: strScaling,
    dex_scaling: dexScaling,
    int_scaling: intScaling,
    fai_scaling: faiScaling,
    arc_scaling: arcScaling,
    base_physical: previewTotalAr,
    base_magic: "0",
    base_fire: "0",
    base_lightning: "0",
    base_holy: "0",
    preview_total_ar: previewTotalAr,
  };
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
  return {
    weaponId,
    weaponName,
    affinity,
    isSomber: options.isSomber ?? false,
    upgrade: options.upgrade ?? 25,
    stats,
    ar: { physical: physicalAr, magic: 0, fire: 0, lightning: 0, holy: 0, total: physicalAr },
    aowId: options.aowId ?? 1,
    aowName: options.aowName === undefined ? "Seppuku" : options.aowName,
    bleedBuildup,
    bleedBuildupAdd: 0,
    frostBuildup: 0,
    poisonBuildup: 0,
    scarletRotBuildup: 0,
    aowFirstHitDamage,
    aowFullSequenceDamage,
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
  catalog: () => call<CatalogDto>("get_catalog"),
  dataManifest: () => call<DataManifestDto>("get_data_manifest"),
  weaponProfile: (weaponName: string, affinity: string | null) =>
    call<WeaponProfileDto>("get_weapon_profile", {
      request: { weaponName, affinity },
    }),
  estimateSearchSpace: (request: OptimizeRequestDto) =>
    call<SearchEstimateDto>("estimate_search_space", { request }),
  runSearch: (request: OptimizeRequestDto) =>
    call<SolvedBuildDto[]>("run_search", { request }),
  startSearch: (request: OptimizeRequestDto) =>
    call<{ jobId: string }>("start_search", { request }),
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
  buildPathPreview: (
    base: OptimizeRequestDto,
    solved: SolvedBuildDto,
    levelsAhead: number,
    title: string,
  ) =>
    call<PathPreviewDto>("build_path_preview", {
      request: { base, solved, levelsAhead, title },
    }),
  buildAffinityWatch: (
    base: OptimizeRequestDto,
    solved: SolvedBuildDto,
    levelsAhead: number,
  ) =>
    call<AffinityWatchPayloadDto>("build_affinity_watch", {
      request: { base, solved, levelsAhead },
    }),
  affinitiesForWeapon: (weaponName: string) =>
    call<string[]>("affinities_for_weapon", { weaponName }),
  compatibleAowNames: (weaponName: string | null, affinity: string | null) =>
    call<string[]>("compatible_aow_names", {
      request: { weaponName, affinity },
    }),
  compatibleAowNamesForAffinity: (affinity: string | null) =>
    call<string[]>("compatible_aow_names_for_affinity", {
      request: { affinity },
    }),
  weaponNamesForType: (weaponTypeKey: string | null) =>
    call<string[]>("weapon_names_for_type", {
      request: { weaponTypeKey },
    }),
  weaponScalingForUpgrade: (weaponName: string, affinity: string, upgrade: number) =>
    call<ScalingDto>("weapon_scaling_for_upgrade", {
      request: { weaponName, affinity, upgrade },
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

async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "get_catalog":
      return await mockCatalog() as T;
    case "get_data_manifest":
      return mockDataManifest() as T;
    case "get_weapon_profile":
      return await mockWeaponProfile(args) as T;
    case "estimate_search_space":
      return await mockSearchEstimate(args) as T;
    case "run_search":
      return await mockRows((args?.request as OptimizeRequestDto | undefined) ?? null) as T;
    case "start_search":
      return { jobId: "browser-preview" } as T;
    case "cancel_search":
      return true as T;
    case "get_search_status":
      return null as T;
    case "solve_build":
      return await mockSolveBuild(args) as T;
    case "build_upgrade_series":
      return await mockUpgradeSeries(args) as T;
    case "build_path_preview": {
      return await mockPathPreview(args) as T;
    }
    case "build_affinity_watch":
      return await mockAffinityWatch(args) as T;
    case "weapon_names_for_type": {
      const key = (args?.request as { weaponTypeKey?: string | null })?.weaponTypeKey;
      return await mockWeaponNamesForType(key ?? null) as T;
    }
    case "compatible_aow_names_for_affinity":
      return await mockCompatibleAowNamesForAffinity(args) as T;
    case "compatible_aow_names":
      return await mockCompatibleAowNames(args) as T;
    case "weapon_scaling_for_upgrade":
      return await mockScalingForUpgrade(args) as T;
    case "affinities_for_weapon":
      return await mockAffinitiesForWeapon(args) as T;
    case "start_path_preview":
      return { jobId: "browser-path-preview" } as T;
    case "cancel_path_preview":
      return true as T;
    case "get_path_preview_status":
      return null as T;
    case "start_affinity_watch":
      return { jobId: "browser-affinity-watch" } as T;
    case "cancel_affinity_watch":
      return true as T;
    case "get_affinity_watch_status":
      return null as T;
    default:
      return null as T;
  }
}

async function mockCatalog(): Promise<CatalogDto> {
  const [weapons, aows] = await Promise.all([phaseWeaponRows(), phaseAowRows()]);
  const weaponNames = uniqueSorted(weapons.map((row) => row.name).filter(Boolean));
  const weaponTypeOptions = weaponTypeOptionsFromRows(weapons);
  return {
    weaponCount: weapons.length,
    aowCount: aows.length,
    weaponNames,
    weaponTypeKeys: weaponTypeOptions.map((entry) => entry.label),
    classes: STARTING_CLASS_METADATA,
    weaponTypeOptions,
    aowNames: uniqueSorted(aows.map((row) => row.name).filter(Boolean)),
    objectiveIds: ["max_ar", "max_physical_ar", "max_ar_plus_bleed", "aow_first_hit", "aow_full_sequence"],
    somberFilters: ["all", "standard_only", "somber_only"],
    dataManifest: mockDataManifest(),
  };
}

function mockDataManifest(): DataManifestDto {
  return {
    id: "phase1-app-1.16.1",
    label: "Phase 1 dataset - App Ver. 1.16.1",
    appVersion: "1.16.1",
    source: "ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx",
    generatedAt: "2026-05-18",
  };
}

async function mockWeaponProfile(args: Record<string, unknown> | undefined): Promise<WeaponProfileDto> {
  const request = args?.request as { weaponName: string; affinity: string | null };
  const [weapons, compatibleAows] = await Promise.all([
    phaseWeaponRows(),
    mockCompatibleAowNames({ request: { weaponName: request.weaponName, affinity: request.affinity } }),
  ]);
  const matches = weapons.filter((row) =>
    row.name === request.weaponName && (!request.affinity || row.affinity === request.affinity),
  );
  const first = matches[0];
  const requirements = matches.reduce(
    (current, row) => ({
      strStat: Math.max(current.strStat, Number(row.req_str || 0)),
      dex: Math.max(current.dex, Number(row.req_dex || 0)),
      intStat: Math.max(current.intStat, Number(row.req_int || 0)),
      fai: Math.max(current.fai, Number(row.req_fai || 0)),
      arc: Math.max(current.arc, Number(row.req_arc || 0)),
    }),
    { strStat: 0, dex: 0, intStat: 0, fai: 0, arc: 0 },
  );
  const maxUpgrade = first?.is_somber === "1" ? 10 : 25;
  return {
    requirements,
    maxUpgrade,
    isSomber: maxUpgrade <= 10,
    disablesTwoHandBonus: matches.some((row) => row.disable_two_hand_bonus === "1"),
    affinities: uniqueSorted(weapons.filter((row) => row.name === request.weaponName).map((row) => row.affinity)),
    compatibleAows,
  };
}

async function mockWeaponNamesForType(weaponTypeKey: string | null): Promise<string[]> {
  const weapons = await phaseWeaponRows();
  return uniqueSorted(
    weapons
      .filter((row) => !weaponTypeKey || weaponTypeMatches(row, weaponTypeKey))
      .map((row) => row.name),
  );
}

async function mockAffinitiesForWeapon(args: Record<string, unknown> | undefined): Promise<string[]> {
  const weaponName = args?.weaponName as string | undefined;
  const weapons = await phaseWeaponRows();
  return uniqueSorted(weapons.filter((row) => row.name === weaponName).map((row) => row.affinity));
}

async function mockCompatibleAowNamesForAffinity(args: Record<string, unknown> | undefined): Promise<string[]> {
  const affinity = (args?.request as { affinity?: string | null } | undefined)?.affinity;
  const rows = await phaseAowAffinityRows();
  return uniqueSorted(rows.filter((row) => !affinity || row.affinity === affinity).map((row) => row.name));
}

async function mockCompatibleAowNames(args: Record<string, unknown> | undefined): Promise<string[]> {
  const request = args?.request as { weaponName?: string | null; affinity?: string | null } | undefined;
  if (!request?.weaponName) {
    return request?.affinity
      ? mockCompatibleAowNamesForAffinity({ request: { affinity: request.affinity } })
      : uniqueSorted((await phaseAowRows()).map((row) => row.name));
  }
  const [weapons, compatRows] = await Promise.all([phaseWeaponRows(), phaseAowWeaponCompatRows()]);
  const nativeSkillNames = weapons
    .filter((row) =>
      row.name === request.weaponName && (!request.affinity || row.affinity === request.affinity),
    )
    .map((row) => row.native_skill_name)
    .filter(Boolean);
  const compatibleNames = compatRows
    .filter((row) =>
      row.weapon_name === request.weaponName && (!request.affinity || row.affinity === request.affinity),
    )
    .map((row) => row.aow_name);
  return uniqueSorted([...nativeSkillNames, ...compatibleNames]);
}

async function mockSearchEstimate(args: Record<string, unknown> | undefined): Promise<SearchEstimateDto> {
  const request = (args?.request as OptimizeRequestDto | undefined) ?? null;
  const weapons = await mockWeaponCandidates(request);
  const upgradeCount = weapons.reduce((total, weapon) => {
    if (!request) return total + 26;
    if (request.exactUpgrade) return total + 1;
    const cap = weapon.is_somber === "1" ? request.somberMaxUpgrade : request.standardMaxUpgrade;
    return total + Math.max(Number(cap) + 1, 1);
  }, 0);
  return {
    weaponCandidates: weapons.length,
    statCandidates: 1,
    combinations: Math.max(upgradeCount, 1),
  };
}

async function mockRows(request: OptimizeRequestDto | null): Promise<SolvedBuildDto[]> {
  const topK = Math.min(Math.max(Number(request?.topK ?? 25), 1), 500);
  return PREVIEW_SOLVED_BUILDS.filter((row) => matchesPreviewRow(row, request)).slice(0, topK);
}

async function mockWeaponCandidates(request: OptimizeRequestDto | null): Promise<CsvRow[]> {
  const [weapons, compatRows] = await Promise.all([phaseWeaponRows(), phaseAowWeaponCompatRows()]);
  return weapons.filter((weapon) => {
    if (request?.weaponName && weapon.name !== request.weaponName) return false;
    if (request?.affinity && weapon.affinity !== request.affinity) return false;
    if (request?.weaponTypeKey && !weaponTypeMatches(weapon, request.weaponTypeKey)) return false;
    if (request?.somberFilter === "standard_only" && weapon.is_somber === "1") return false;
    if (request?.somberFilter === "somber_only" && weapon.is_somber !== "1") return false;
    if (request?.aowName && !mockAowCompatible(weapon, request.aowName, compatRows)) return false;
    return true;
  });
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

function mockAowCompatible(weapon: CsvRow, aowName: string, compatRows: CsvRow[]): boolean {
  return weapon.native_skill_name === aowName
    || compatRows.some((row) => row.weapon_id === weapon.weapon_id && row.aow_name === aowName);
}

function matchesPreviewRow(row: SolvedBuildDto, request: OptimizeRequestDto | null): boolean {
  if (!request) return true;
  if (request.weaponName && row.weaponName !== request.weaponName) return false;
  if (request.affinity && row.affinity !== request.affinity) return false;
  if (request.aowName && row.aowName !== request.aowName) return false;
  if (request.weaponTypeKey) {
    const weapon = FIXTURE_WEAPONS.find((entry) => entry.name === row.weaponName && entry.affinity === row.affinity);
    if (!weapon || !weaponTypeMatches(weapon, request.weaponTypeKey)) return false;
  }
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
    return { title: request?.title ?? "Path", solved: emptySolvedBuild(), steps: [] };
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

function emptySolvedBuild(): SolvedBuildDto {
  return {
    weaponId: 0,
    weaponName: "",
    affinity: "",
    isSomber: false,
    upgrade: 0,
    stats: { strStat: 0, dex: 0, intStat: 0, fai: 0, arc: 0 },
    ar: { physical: 0, magic: 0, fire: 0, lightning: 0, holy: 0, total: 0 },
    aowId: null,
    aowName: null,
    bleedBuildup: 0,
    bleedBuildupAdd: 0,
    frostBuildup: 0,
    poisonBuildup: 0,
    scarletRotBuildup: 0,
    aowFirstHitDamage: 0,
    aowFullSequenceDamage: 0,
    score: 0,
  };
}

async function mockScalingForUpgrade(args: Record<string, unknown> | undefined): Promise<ScalingDto> {
  const request = args?.request as {
    weaponName?: string;
    affinity?: string;
    upgrade?: number;
  } | undefined;
  const weapons = await phaseWeaponRows();
  const weapon = weapons.find(
    (row) => row.name === request?.weaponName && row.affinity === request?.affinity,
  );
  if (!weapon) {
    return { str: 0, dex: 0, int: 0, fai: 0, arc: 0 };
  }
  return {
    str: Number(weapon.str_scaling),
    dex: Number(weapon.dex_scaling),
    int: Number(weapon.int_scaling),
    fai: Number(weapon.fai_scaling),
    arc: Number(weapon.arc_scaling),
  };
}

function fixed1(value: number): number {
  return Number(value.toFixed(1));
}

function phaseWeaponRows(): Promise<CsvRow[]> {
  return Promise.resolve(FIXTURE_WEAPONS);
}

function phaseAowRows(): Promise<CsvRow[]> {
  return Promise.resolve(FIXTURE_AOWS);
}

function phaseAowAffinityRows(): Promise<CsvRow[]> {
  return Promise.resolve(FIXTURE_AOW_AFFINITIES);
}

function phaseAowWeaponCompatRows(): Promise<CsvRow[]> {
  return Promise.resolve(FIXTURE_AOW_WEAPON_COMPAT);
}

function weaponTypeOptionsFromRows(weapons: CsvRow[]): Array<{ key: string; label: string }> {
  const options = new Map<string, { key: string; label: string }>();
  for (const weapon of weapons) {
    const label = normalizeWeaponTypeDisplay(weapon.weapon_type_name);
    if (!label || options.has(label)) {
      continue;
    }
    const keys = weapon.weapon_type_keys.split("|").map((key) => key.trim()).filter(Boolean);
    const key = keys.find((candidate) => candidate.toLocaleLowerCase() === label.toLocaleLowerCase()) ?? keys[0] ?? label;
    options.set(label, { key, label });
  }
  return Array.from(options.values()).sort((left, right) => left.label.localeCompare(right.label));
}

function weaponTypeMatches(weapon: CsvRow, weaponTypeKey: string): boolean {
  return normalizeWeaponTypeDisplay(weapon.weapon_type_name).toLocaleLowerCase() === weaponTypeKey.toLocaleLowerCase()
    || weapon.weapon_type_name.toLocaleLowerCase() === weaponTypeKey.toLocaleLowerCase()
    || weapon.weapon_type_keys
      .split("|")
      .some((candidate) => candidate.toLocaleLowerCase() === weaponTypeKey.toLocaleLowerCase());
}

function normalizeWeaponTypeDisplay(raw: string): string {
  switch (raw.trim()) {
    case "Hand-to-Hand":
      return "Hand-to-Hand Arts";
    case "Heavy Spear":
      return "Great Spear";
    case "Reverse-hand Blade":
      return "Backhand Blade";
    case "Scythe":
      return "Reaper";
    case "Seal":
      return "Sacred Seal";
    case "Staff":
      return "Glintstone Staff";
    default:
      return raw.trim();
  }
}

function uniqueSorted(values: string[]): string[] {
  return Array.from(new Set(values.filter(Boolean))).sort((left, right) => left.localeCompare(right));
}
