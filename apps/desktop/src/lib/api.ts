import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AffinityWatchPayloadDto,
  AffinityWatchFinishedDto,
  AffinityWatchProgressDto,
  CatalogDto,
  OptimizeRequestDto,
  PathFinishedDto,
  PathPreviewDto,
  PathProgressDto,
  SearchFinishedDto,
  SearchEstimateDto,
  SearchProgressDto,
  ScalingDto,
  SolvedBuildDto,
  UpgradePointDto,
  WeaponProfileDto,
} from "./types";
import { STARTING_CLASS_METADATA } from "./session";

type CsvRow = Record<string, string>;
type CombatStatKey = keyof SolvedBuildDto["stats"];
type PreviewEvaluation = {
  row: SolvedBuildDto | null;
  stats: SolvedBuildDto["stats"];
  requirementGap: number;
};

const COMBAT_STAT_KEYS: CombatStatKey[] = ["strStat", "dex", "intStat", "fai", "arc"];

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
  onSearchProgress: (handler: (payload: SearchProgressDto) => void) =>
    hasTauriRuntime()
      ? listen<SearchProgressDto>("search_progress", (event) => handler(event.payload))
      : Promise.resolve(() => undefined),
  onSearchFinished: (handler: (payload: SearchFinishedDto) => void) =>
    hasTauriRuntime()
      ? listen<SearchFinishedDto>("search_finished", (event) => handler(event.payload))
      : Promise.resolve(() => undefined),
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
  onPathProgress: (handler: (payload: PathProgressDto) => void) =>
    hasTauriRuntime()
      ? listen<PathProgressDto>("path_progress", (event) => handler(event.payload))
      : Promise.resolve(() => undefined),
  onPathFinished: (handler: (payload: PathFinishedDto) => void) =>
    hasTauriRuntime()
      ? listen<PathFinishedDto>("path_finished", (event) => handler(event.payload))
      : Promise.resolve(() => undefined),
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
  onAffinityWatchProgress: (handler: (payload: AffinityWatchProgressDto) => void) =>
    hasTauriRuntime()
      ? listen<AffinityWatchProgressDto>("affinity_watch_progress", (event) => handler(event.payload))
      : Promise.resolve(() => undefined),
  onAffinityWatchFinished: (handler: (payload: AffinityWatchFinishedDto) => void) =>
    hasTauriRuntime()
      ? listen<AffinityWatchFinishedDto>("affinity_watch_finished", (event) => handler(event.payload))
      : Promise.resolve(() => undefined),
};

async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "get_catalog":
      return await mockCatalog() as T;
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
    case "start_affinity_watch":
      return { jobId: "browser-affinity-watch" } as T;
    case "cancel_affinity_watch":
      return true as T;
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
    objectiveIds: ["max_ar", "max_ar_plus_bleed", "aow_first_hit", "aow_full_sequence"],
    somberFilters: ["all", "standard_only", "somber_only"],
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
  const upgradeCount = request?.fixedUpgrade === null ? Number(request?.maxUpgrade ?? 25) + 1 : 1;
  const statCandidates = request ? Math.max(1, request.characterLevel - 1) * 32 : 18496;
  return {
    weaponCandidates: weapons.length,
    statCandidates,
    combinations: weapons.length * Math.max(upgradeCount, 1) * statCandidates,
  };
}

async function mockRows(request: OptimizeRequestDto | null): Promise<SolvedBuildDto[]> {
  const [weapons, reinforceRows, passiveRows, compatRows] = await Promise.all([
    mockWeaponCandidates(request),
    phaseReinforceRows(),
    phaseWeaponPassiveRows(),
    phaseAowWeaponCompatRows(),
  ]);
  const topK = Math.min(Math.max(Number(request?.topK ?? 25), 1), 50);
  const rows = weapons
    .map((weapon) => buildMockRow(weapon, request, reinforceRows, passiveRows, compatRows))
    .filter((row): row is SolvedBuildDto => row !== null)
    .sort((left, right) => metricPreview(right, request?.objective ?? "max_ar") - metricPreview(left, request?.objective ?? "max_ar"));
  return rows.slice(0, topK);
}

async function mockWeaponCandidates(request: OptimizeRequestDto | null): Promise<CsvRow[]> {
  const [weapons, compatRows] = await Promise.all([phaseWeaponRows(), phaseAowWeaponCompatRows()]);
  return weapons.filter((weapon) => {
    if (request?.weaponName && weapon.name !== request.weaponName) return false;
    if (request?.affinity && weapon.affinity !== request.affinity) return false;
    if (request?.weaponTypeKey && !weaponTypeMatches(weapon, request.weaponTypeKey)) return false;
    if (request?.somberFilter === "standard_only" && weapon.is_somber === "1") return false;
    if (request?.somberFilter === "somber_only" && weapon.is_somber !== "1") return false;
    if (!meetsPreviewRequirements(weapon, request)) return false;
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

function buildMockRow(
  weapon: CsvRow,
  request: OptimizeRequestDto | null,
  reinforceRows: CsvRow[],
  passiveRows: CsvRow[],
  compatRows: CsvRow[],
): SolvedBuildDto | null {
  const stats = previewStats(weapon, request);
  if (requirementGapForStats(weapon, request, stats) > 0) {
    return null;
  }
  const cap = weapon.is_somber === "1" ? 10 : 25;
  const requestedUpgrade = request?.fixedUpgrade ?? request?.maxUpgrade ?? cap;
  const upgrade = Math.max(0, Math.min(Number(requestedUpgrade), cap));
  const reinforce = reinforceRows.find(
    (row) => row.reinforce_type === weapon.reinforce_type && Number(row.level) === upgrade,
  );
  const ar = previewAr(weapon, reinforce, stats);
  const aow = choosePreviewAow(weapon, request?.aowName ?? null, compatRows);
  if (request?.aowName && !aow) {
    return null;
  }
  const passive = passiveRows.find((row) => row.weapon_id === weapon.weapon_id);
  const bleedBuildup = Number(passive?.bleed ?? 0);
  const frostBuildup = Number(passive?.frost ?? 0);
  const poisonBuildup = Number(passive?.poison ?? 0);
  const scarletRotBuildup = Number(passive?.scarlet_rot ?? 0);
  const aowFirstHitDamage = Math.round(ar.total * (aow ? 1.22 : 0));
  const aowFullSequenceDamage = Math.round(ar.total * (aow ? 3.15 : 0));
  const score = scorePreview(
    request?.objective ?? "max_ar",
    ar.total,
    bleedBuildup,
    aowFirstHitDamage,
    aowFullSequenceDamage,
  );
  return {
    weaponId: Number(weapon.weapon_id),
    weaponName: weapon.name,
    affinity: weapon.affinity,
    isSomber: weapon.is_somber === "1",
    upgrade,
    stats,
    ar,
    aowId: aow?.id ?? null,
    aowName: aow?.name ?? null,
    bleedBuildup,
    bleedBuildupAdd: 0,
    frostBuildup,
    poisonBuildup,
    scarletRotBuildup,
    aowFirstHitDamage,
    aowFullSequenceDamage,
    score,
  };
}

function previewStats(weapon: CsvRow, request: OptimizeRequestDto | null): SolvedBuildDto["stats"] {
  const current = {
    strStat: Number(request?.lockStr ?? request?.strStat ?? 0),
    dex: Number(request?.lockDex ?? request?.dex ?? 0),
    intStat: Number(request?.lockInt ?? request?.intStat ?? 0),
    fai: Number(request?.lockFai ?? request?.fai ?? 0),
    arc: Number(request?.lockArc ?? request?.arc ?? 0),
  };
  if (!request || hasLockedCombatStats(request)) {
    return current;
  }
  const available = availableCombatPoints(request);
  const stats = { ...current };
  const floors = requirementAwareFloors(weapon, request, stats);
  let spent = 0;
  for (const key of COMBAT_STAT_KEYS) {
    const raised = Math.max(stats[key], floors[key]);
    spent += Math.max(0, raised - stats[key]);
    stats[key] = Math.min(99, raised);
  }
  if (spent > available) {
    return current;
  }
  let remaining = available - spent;
  const weights = statWeights(weapon);
  while (remaining > 0) {
    const key = COMBAT_STAT_KEYS
      .filter((candidate) => stats[candidate] < 99)
      .sort((left, right) => weights[right] - weights[left])[0];
    if (!key || weights[key] <= 0) {
      break;
    }
    stats[key] += 1;
    remaining -= 1;
  }
  return stats;
}

function previewAr(
  weapon: CsvRow,
  reinforce: CsvRow | undefined,
  stats: SolvedBuildDto["stats"],
): SolvedBuildDto["ar"] {
  const physical = previewElementAr(weapon, reinforce, "physical", stats);
  const magic = previewElementAr(weapon, reinforce, "magic", stats);
  const fire = previewElementAr(weapon, reinforce, "fire", stats);
  const lightning = previewElementAr(weapon, reinforce, "lightning", stats);
  const holy = previewElementAr(weapon, reinforce, "holy", stats);
  return {
    physical,
    magic,
    fire,
    lightning,
    holy,
    total: physical + magic + fire + lightning + holy,
  };
}

function previewElementAr(
  weapon: CsvRow,
  reinforce: CsvRow | undefined,
  element: "physical" | "magic" | "fire" | "lightning" | "holy",
  stats: SolvedBuildDto["stats"],
): number {
  const base = Number(weapon[`base_${element}`] || 0);
  if (base <= 0) {
    return 0;
  }
  const damageMult = Number(reinforce?.[`${element}_damage_mult`] ?? 1);
  const statBonus =
    statContribution(stats.strStat, weapon.str_scaling, reinforce?.str_scaling_mult) +
    statContribution(stats.dex, weapon.dex_scaling, reinforce?.dex_scaling_mult) +
    statContribution(stats.intStat, weapon.int_scaling, reinforce?.int_scaling_mult) +
    statContribution(stats.fai, weapon.fai_scaling, reinforce?.fai_scaling_mult) +
    statContribution(stats.arc, weapon.arc_scaling, reinforce?.arc_scaling_mult);
  return Math.round(base * damageMult * (1 + statBonus));
}

function statContribution(stat: number, scaling: string, multiplier = "1"): number {
  const aboveBase = Math.max(0, Math.min(stat, 99) - 10);
  return Number(scaling || 0) * Number(multiplier || 1) * aboveBase / 220;
}

function hasLockedCombatStats(request: OptimizeRequestDto): boolean {
  return [request.lockStr, request.lockDex, request.lockInt, request.lockFai, request.lockArc]
    .some((value) => value !== null);
}

function availableCombatPoints(request: OptimizeRequestDto): number {
  const meta = STARTING_CLASS_METADATA.find((entry) => entry.name === request.className);
  if (!meta) {
    return 0;
  }
  const currentTotal =
    request.vig + request.mnd + request.end +
    request.strStat + request.dex + request.intStat + request.fai + request.arc;
  const levelTotal = meta.baseTotal + (request.characterLevel - meta.baseLevel);
  return Math.max(0, levelTotal - currentTotal);
}

function requirementAwareFloors(
  weapon: CsvRow,
  request: OptimizeRequestDto,
  current: SolvedBuildDto["stats"],
): SolvedBuildDto["stats"] {
  const strRequirement = Number(weapon.req_str || 0);
  const requiredStr = request.twoHanding && weapon.disable_two_hand_bonus !== "1"
    ? Math.ceil(strRequirement / 1.5)
    : strRequirement;
  return {
    strStat: Math.max(current.strStat, request.minStr, requiredStr),
    dex: Math.max(current.dex, request.minDex, Number(weapon.req_dex || 0)),
    intStat: Math.max(current.intStat, request.minInt, Number(weapon.req_int || 0)),
    fai: Math.max(current.fai, request.minFai, Number(weapon.req_fai || 0)),
    arc: Math.max(current.arc, request.minArc, Number(weapon.req_arc || 0)),
  };
}

function statWeights(weapon: CsvRow): Record<CombatStatKey, number> {
  return {
    strStat: Number(weapon.str_scaling || 0),
    dex: Number(weapon.dex_scaling || 0),
    intStat: Number(weapon.int_scaling || 0),
    fai: Number(weapon.fai_scaling || 0),
    arc: Number(weapon.arc_scaling || 0),
  };
}

function requirementGapForStats(
  weapon: CsvRow,
  request: OptimizeRequestDto | null,
  stats: SolvedBuildDto["stats"],
): number {
  const effectiveStr = request?.twoHanding && weapon.disable_two_hand_bonus !== "1"
    ? Math.min(99, Math.floor(stats.strStat * 1.5))
    : stats.strStat;
  return Math.max(Number(weapon.req_str || 0) - effectiveStr, 0)
    + Math.max(Number(weapon.req_dex || 0) - stats.dex, 0)
    + Math.max(Number(weapon.req_int || 0) - stats.intStat, 0)
    + Math.max(Number(weapon.req_fai || 0) - stats.fai, 0)
    + Math.max(Number(weapon.req_arc || 0) - stats.arc, 0);
}

function choosePreviewAow(
  weapon: CsvRow,
  requestedAow: string | null,
  compatRows: CsvRow[],
): { id: number | null; name: string } | null {
  if (requestedAow) {
    if (weapon.native_skill_name === requestedAow) {
      return { id: Number(weapon.native_skill_id || 0) || null, name: requestedAow };
    }
    const compat = compatRows.find((row) => row.weapon_id === weapon.weapon_id && row.aow_name === requestedAow);
    return compat ? { id: Number(compat.aow_id || 0) || null, name: requestedAow } : null;
  }
  if (weapon.native_skill_name) {
    return { id: Number(weapon.native_skill_id || 0) || null, name: weapon.native_skill_name };
  }
  const compat = compatRows.find((row) => row.weapon_id === weapon.weapon_id);
  return compat ? { id: Number(compat.aow_id || 0) || null, name: compat.aow_name } : null;
}

function mockAowCompatible(weapon: CsvRow, aowName: string, compatRows: CsvRow[]): boolean {
  return weapon.native_skill_name === aowName
    || compatRows.some((row) => row.weapon_id === weapon.weapon_id && row.aow_name === aowName);
}

function meetsPreviewRequirements(weapon: CsvRow, request: OptimizeRequestDto | null): boolean {
  if (!request) {
    return true;
  }
  return requirementGapForStats(weapon, request, previewStats(weapon, request)) === 0;
}

function scorePreview(
  objective: OptimizeRequestDto["objective"],
  totalAr: number,
  bleed: number,
  aowFirstHitDamage: number,
  aowFullSequenceDamage: number,
): number {
  switch (objective) {
    case "aow_first_hit":
      return aowFirstHitDamage;
    case "aow_full_sequence":
      return aowFullSequenceDamage;
    case "max_ar_plus_bleed":
      return totalAr + bleed;
    default:
      return totalAr;
  }
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
  const base = request?.base;
  const points: UpgradePointDto[] = [];
  for (let upgrade = 0; upgrade <= maxUpgrade; upgrade += 1) {
    const rows = await mockRows({
      ...base,
      weaponName: row.weaponName,
      affinity: row.affinity,
      aowName: row.aowName,
      weaponTypeKey: null,
      somberFilter: "all",
      minStr: 0,
      minDex: 0,
      minInt: 0,
      minFai: 0,
      minArc: 0,
      maxUpgrade: upgrade,
      fixedUpgrade: upgrade,
      lockStr: row.stats.strStat,
      lockDex: row.stats.dex,
      lockInt: row.stats.intStat,
      lockFai: row.stats.fai,
      lockArc: row.stats.arc,
      topK: 1,
    } as OptimizeRequestDto);
    const solved = rows[0];
    if (solved) {
      points.push({ upgrade, metric: metricPreview(solved, base?.objective ?? "max_ar") });
    }
  }
  return points;
}

function metricPreview(row: SolvedBuildDto, objective: OptimizeRequestDto["objective"]): number {
  switch (objective) {
    case "aow_first_hit":
      return row.aowFirstHitDamage;
    case "aow_full_sequence":
      return row.aowFullSequenceDamage;
    case "max_ar_plus_bleed":
      return row.score;
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
  const steps = [await evaluatePathStep(base, solved, base.characterLevel, solved.stats, null)];
  const target = await pathTargetBuild(base, solved, levelsAhead);
  if (!target) {
    return { title: request?.title ?? "Path", solved, steps };
  }
  let current = solved.stats;
  for (let delta = 1; delta <= levelsAhead; delta += 1) {
    const level = base.characterLevel + delta;
    const candidates = [];
    for (const stat of COMBAT_STAT_KEYS) {
      if (current[stat] >= target.stats[stat] || current[stat] >= 99) {
        continue;
      }
      const nextStats = { ...current, [stat]: current[stat] + 1 };
      candidates.push(await evaluatePathStep(base, solved, level, nextStats, statName(stat)));
    }
    candidates.sort(comparePathSteps);
    const next = candidates.at(-1);
    if (!next) {
      break;
    }
    current = next.stats;
    steps.push(next);
  }
  return { title: request?.title ?? "Path", solved, steps };
}

async function pathTargetBuild(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  levelsAhead: number,
): Promise<SolvedBuildDto | null> {
  const targetLevel = base.characterLevel + levelsAhead;
  const rows = await mockRows({
    ...base,
    characterLevel: targetLevel,
    weaponName: solved.weaponName,
    affinity: solved.affinity,
    aowName: solved.aowName,
    maxUpgrade: solved.upgrade,
    fixedUpgrade: solved.upgrade,
    weaponTypeKey: null,
    somberFilter: "all",
    minStr: Math.max(base.minStr, solved.stats.strStat),
    minDex: Math.max(base.minDex, solved.stats.dex),
    minInt: Math.max(base.minInt, solved.stats.intStat),
    minFai: Math.max(base.minFai, solved.stats.fai),
    minArc: Math.max(base.minArc, solved.stats.arc),
    lockStr: null,
    lockDex: null,
    lockInt: null,
    lockFai: null,
    lockArc: null,
    topK: 1,
  });
  return rows[0] ?? null;
}

async function evaluatePathStep(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  level: number,
  stats: SolvedBuildDto["stats"],
  addedStat: string | null,
): Promise<{
  level: number;
  stats: SolvedBuildDto["stats"];
  metric: number | null;
  score: number | null;
  addedStat: string | null;
  requirementGap: number;
}> {
  const request = {
    ...base,
    characterLevel: level,
    weaponName: solved.weaponName,
    affinity: solved.affinity,
    aowName: solved.aowName,
    maxUpgrade: solved.upgrade,
    fixedUpgrade: solved.upgrade,
    weaponTypeKey: null,
    somberFilter: "all",
    minStr: 0,
    minDex: 0,
    minInt: 0,
    minFai: 0,
    minArc: 0,
    lockStr: stats.strStat,
    lockDex: stats.dex,
    lockInt: stats.intStat,
    lockFai: stats.fai,
    lockArc: stats.arc,
    topK: 1,
  } as OptimizeRequestDto;
  const row = (await mockRows(request))[0] ?? null;
  return {
    level,
    stats,
    metric: row ? metricPreview(row, base.objective) : null,
    score: row?.score ?? null,
    addedStat,
    requirementGap: row ? 0 : await requirementGapForSolved(base, solved, stats),
  };
}

async function requirementGapForSolved(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  stats: SolvedBuildDto["stats"],
): Promise<number> {
  const weapons = await phaseWeaponRows();
  const weapon = weapons.find((row) =>
    row.name === solved.weaponName && row.affinity === solved.affinity,
  );
  return weapon ? requirementGapForStats(weapon, base, stats) : 999;
}

function comparePathSteps(
  left: Awaited<ReturnType<typeof evaluatePathStep>>,
  right: Awaited<ReturnType<typeof evaluatePathStep>>,
): number {
  return pathStepKey(left).localeCompare(pathStepKey(right));
}

function pathStepKey(step: Awaited<ReturnType<typeof evaluatePathStep>>): string {
  return [
    step.metric !== null && step.score !== null ? 1 : 0,
    padScore(step.score ?? 0),
    padScore(step.metric ?? 0),
    padScore(-step.requirementGap),
    padScore(-statPriority(step.addedStat)),
  ].join("|");
}

function padScore(value: number): string {
  return (value + 1000000).toFixed(4).padStart(16, "0");
}

function statPriority(stat: string | null): number {
  return ["str", "dex", "int", "fai", "arc", null].indexOf(stat);
}

function statName(key: CombatStatKey): string {
  switch (key) {
    case "strStat":
      return "str";
    case "intStat":
      return "int";
    default:
      return key;
  }
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
  let affinities = await mockAffinitiesForWeapon({ weaponName: solved.weaponName });
  if (solved.aowName) {
    const pairs = await Promise.all(
      affinities.map(async (affinity) => ({
        affinity,
        aows: await mockCompatibleAowNames({ request: { weaponName: solved.weaponName, affinity } }),
      })),
    );
    affinities = pairs
      .filter((entry) => entry.aows.includes(solved.aowName ?? ""))
      .map((entry) => entry.affinity);
  }
  if (!affinities.includes(solved.affinity)) {
    affinities.push(solved.affinity);
  }
  affinities.sort((left, right) =>
    Number(left !== solved.affinity) - Number(right !== solved.affinity) || left.localeCompare(right),
  );
  const levels = Array.from(
    { length: Math.max(0, Math.trunc(Number(request.levelsAhead ?? 0))) + 1 },
    (_, index) => base.characterLevel + index,
  );
  const lines = [];
  for (const affinity of affinities) {
    const points = [];
    for (const level of levels) {
      const rows = await mockRows({
        ...base,
        characterLevel: level,
        weaponName: solved.weaponName,
        affinity,
        aowName: solved.aowName,
        maxUpgrade: solved.upgrade,
        fixedUpgrade: solved.upgrade,
        weaponTypeKey: null,
        somberFilter: "all",
        lockStr: null,
        lockDex: null,
        lockInt: null,
        lockFai: null,
        lockArc: null,
        topK: 1,
      });
      const row = rows[0] ?? null;
      points.push({
        level,
        metric: row ? metricPreview(row, base.objective) : null,
        solved: row,
      });
    }
    const valid = points.filter((point) => point.metric !== null && point.solved !== null);
    if (valid.length === 0) {
      continue;
    }
    lines.push({
      affinity,
      points,
      startMetric: valid[0]?.metric ?? null,
      endMetric: valid.at(-1)?.metric ?? null,
      finalBuild: valid.at(-1)?.solved ?? null,
    });
  }
  lines.sort((left, right) => compareAffinityLines(right, left, base.objective));
  return {
    lines,
    breakpoints: detectPreviewBreakpoints(lines, levels, base.objective),
  };
}

function compareAffinityLines(
  left: AffinityWatchPayloadDto["lines"][number],
  right: AffinityWatchPayloadDto["lines"][number],
  objective: OptimizeRequestDto["objective"],
): number {
  const leftMetric = left.endMetric ?? Number.NEGATIVE_INFINITY;
  const rightMetric = right.endMetric ?? Number.NEGATIVE_INFINITY;
  if (leftMetric !== rightMetric) {
    return leftMetric - rightMetric;
  }
  if (left.finalBuild && right.finalBuild) {
    return resultRankValue(left.finalBuild, objective) - resultRankValue(right.finalBuild, objective);
  }
  return Number(Boolean(left.finalBuild)) - Number(Boolean(right.finalBuild));
}

function detectPreviewBreakpoints(
  lines: AffinityWatchPayloadDto["lines"],
  levels: number[],
  objective: OptimizeRequestDto["objective"],
): AffinityWatchPayloadDto["breakpoints"] {
  const breakpoints = [];
  let leaderAffinity: string | null = null;
  for (const level of levels) {
    const contenders = lines
      .map((line) => line.points.find((point) => point.level === level)?.solved ?? null)
      .filter((row): row is SolvedBuildDto => row !== null);
    contenders.sort((left, right) => resultRankValue(right, objective) - resultRankValue(left, objective));
    const leader = contenders[0];
    if (!leader) {
      continue;
    }
    if (leaderAffinity && leaderAffinity !== leader.affinity) {
      breakpoints.push({
        level,
        outgoingAffinity: leaderAffinity,
        incomingAffinity: leader.affinity,
        outgoingMetric: metricAt(lines, leaderAffinity, level),
        incomingMetric: metricAt(lines, leader.affinity, level),
      });
    }
    leaderAffinity = leader.affinity;
  }
  return breakpoints;
}

function metricAt(lines: AffinityWatchPayloadDto["lines"], affinity: string, level: number): number | null {
  return lines
    .find((line) => line.affinity === affinity)
    ?.points.find((point) => point.level === level)
    ?.metric ?? null;
}

function resultRankValue(row: SolvedBuildDto, objective: OptimizeRequestDto["objective"]): number {
  return metricPreview(row, objective) * 1_000_000
    + row.score * 1_000
    + row.ar.total
    + row.aowFullSequenceDamage / 1_000
    + row.aowFirstHitDamage / 10_000
    + row.bleedBuildup / 100_000
    - row.weaponId / 1_000_000_000;
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
  const [weapons, reinforceRows] = await Promise.all([phaseWeaponRows(), phaseReinforceRows()]);
  const weapon = weapons.find(
    (row) => row.name === request?.weaponName && row.affinity === request?.affinity,
  );
  if (!weapon) {
    return { str: 0, dex: 0, int: 0, fai: 0, arc: 0 };
  }
  const reinforce = reinforceRows.find(
    (row) => row.reinforce_type === weapon.reinforce_type && Number(row.level) === Number(request?.upgrade ?? 0),
  );
  return {
    str: scaleValue(weapon.str_scaling, reinforce?.str_scaling_mult),
    dex: scaleValue(weapon.dex_scaling, reinforce?.dex_scaling_mult),
    int: scaleValue(weapon.int_scaling, reinforce?.int_scaling_mult),
    fai: scaleValue(weapon.fai_scaling, reinforce?.fai_scaling_mult),
    arc: scaleValue(weapon.arc_scaling, reinforce?.arc_scaling_mult),
  };
}

function scaleValue(base: string, multiplier = "1"): number {
  return Number(base) * Number(multiplier);
}

let cachedWeaponRows: Promise<CsvRow[]> | null = null;
let cachedReinforceRows: Promise<CsvRow[]> | null = null;
let cachedAowRows: Promise<CsvRow[]> | null = null;
let cachedAowAffinityRows: Promise<CsvRow[]> | null = null;
let cachedAowWeaponCompatRows: Promise<CsvRow[]> | null = null;
let cachedWeaponPassiveRows: Promise<CsvRow[]> | null = null;

function phaseWeaponRows(): Promise<CsvRow[]> {
  cachedWeaponRows ??= import("../../../../data/phase1/weapons.csv?raw")
    .then((module) => parseCsv(module.default));
  return cachedWeaponRows;
}

function phaseReinforceRows(): Promise<CsvRow[]> {
  cachedReinforceRows ??= import("../../../../data/phase1/reinforce.csv?raw")
    .then((module) => parseCsv(module.default));
  return cachedReinforceRows;
}

function phaseAowRows(): Promise<CsvRow[]> {
  cachedAowRows ??= import("../../../../data/phase1/aow.csv?raw")
    .then((module) => parseCsv(module.default));
  return cachedAowRows;
}

function phaseAowAffinityRows(): Promise<CsvRow[]> {
  cachedAowAffinityRows ??= import("../../../../data/phase1/aow_affinity_compat.csv?raw")
    .then((module) => parseCsv(module.default));
  return cachedAowAffinityRows;
}

function phaseAowWeaponCompatRows(): Promise<CsvRow[]> {
  cachedAowWeaponCompatRows ??= import("../../../../data/phase1/aow_weapon_compat.csv?raw")
    .then((module) => parseCsv(module.default));
  return cachedAowWeaponCompatRows;
}

function phaseWeaponPassiveRows(): Promise<CsvRow[]> {
  cachedWeaponPassiveRows ??= import("../../../../data/phase1/weapon_passives.csv?raw")
    .then((module) => parseCsv(module.default));
  return cachedWeaponPassiveRows;
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

function parseCsv(text: string): CsvRow[] {
  const [headerLine, ...lines] = text.trim().split(/\r?\n/);
  const headers = splitCsvLine(headerLine);
  return lines.map((line) => Object.fromEntries(
    splitCsvLine(line).map((value, index) => [headers[index], value]),
  ));
}

function splitCsvLine(line: string): string[] {
  const values: string[] = [];
  let value = "";
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    if (char === '"') {
      if (quoted && line[index + 1] === '"') {
        value += '"';
        index += 1;
      } else {
        quoted = !quoted;
      }
    } else if (char === "," && !quoted) {
      values.push(value);
      value = "";
    } else {
      value += char;
    }
  }
  values.push(value);
  return values;
}
