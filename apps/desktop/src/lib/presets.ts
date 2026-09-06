import { STARTING_CLASS_METADATA } from "./session";
import { BuildPreset, BuildPresetV1, OptimizeRequestDto, SavedBuildIndexV1, SolvedBuildDto } from "./types";

const INDEX_KEY = "tarnisheds-arsenal.savedBuildIndex.v1";
const PRESET_PREFIX = "tarnisheds-arsenal.savedBuild.v2.";
const LEGACY_PRESET_PREFIX = "tarnisheds-arsenal.savedBuild.v1.";
export const SHARE_PREFIX = "ta-v2:";
const LEGACY_SHARE_PREFIX = "ta-v1:";
export const MAX_PRESET_IMPORT_BYTES = 256 * 1024;

export interface PresetImportPreview {
  preset: BuildPreset;
  bytes: number;
  idConflict: boolean;
  nameConflict: boolean;
}

export function savedBuildIndex(): SavedBuildIndexV1 {
  const value = readJson<unknown>(INDEX_KEY);
  if (!isRecord(value) || value.version !== 1 || !Array.isArray(value.builds)) {
    return { version: 1, builds: [] };
  }
  const builds = value.builds.filter((entry): entry is SavedBuildIndexV1["builds"][number] => {
    if (!isRecord(entry)) return false;
    try {
      assertText(entry.id, "saved index id", 128);
      assertText(entry.name, "saved index name", 200);
      if (typeof entry.profileId !== "string") entry.profileId = "vanilla";
      assertProfileId(entry.profileId, "saved index profileId");
      assertDataVersion(entry.dataVersion);
      assertDate(entry.updatedAt, "saved index updatedAt");
      return true;
    } catch {
      return false;
    }
  });
  return { version: 1, builds };
}

export function loadBuildPreset(id: string): BuildPreset | null {
  const raw = readJson<unknown>(`${PRESET_PREFIX}${id}`) ?? readJson<unknown>(`${LEGACY_PRESET_PREFIX}${id}`);
  try {
    const preset = migratePreset(raw);
    assertPreset(preset);
    return preset;
  } catch {
    return null;
  }
}

export function saveBuildPreset(input: {
  id?: string;
  name: string;
  request: OptimizeRequestDto;
  selectedBuild: SolvedBuildDto | null;
  compareTarget: SolvedBuildDto | null;
  compareBench?: SolvedBuildDto[];
  dataVersion: string;
}): BuildPreset {
  const now = new Date().toISOString();
  const existing = input.id ? loadBuildPreset(input.id) : null;
  const preset: BuildPreset = {
    version: 2,
    id: input.id ?? crypto.randomUUID(),
    name: input.name.trim() || "Untitled Build",
    profileId: input.request.profileId,
    request: input.request,
    selectedBuild: input.selectedBuild,
    compareTarget: input.compareTarget,
    compareBench: input.compareBench ?? [],
    dataVersion: input.dataVersion,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };
  assertPreset(preset);
  persistPreset(preset);
  return preset;
}

export function renameBuildPreset(id: string, name: string): BuildPreset {
  const preset = loadBuildPreset(id);
  if (!preset) {
    throw new Error("Saved build was not found.");
  }
  const renamed = { ...preset, name: name.trim() || preset.name, updatedAt: new Date().toISOString() };
  persistPreset(renamed);
  return renamed;
}

export function deleteBuildPreset(id: string) {
  const index = savedBuildIndex();
  localStorage.setItem(INDEX_KEY, JSON.stringify({
    version: 1,
    builds: index.builds.filter((entry) => entry.id !== id),
  } satisfies SavedBuildIndexV1));
  localStorage.removeItem(`${PRESET_PREFIX}${id}`);
  localStorage.removeItem(`${LEGACY_PRESET_PREFIX}${id}`);
}

export function parsePresetText(raw: string): BuildPreset {
  const text = raw.trim();
  const bytes = new TextEncoder().encode(text).byteLength;
  if (bytes > MAX_PRESET_IMPORT_BYTES) {
    throw new Error(`Preset is too large (${bytes.toLocaleString()} bytes; limit ${MAX_PRESET_IMPORT_BYTES.toLocaleString()}).`);
  }
  const prefix = text.startsWith(SHARE_PREFIX) ? SHARE_PREFIX : text.startsWith(LEGACY_SHARE_PREFIX) ? LEGACY_SHARE_PREFIX : "";
  const payload = prefix ? decodeURIComponent(text.slice(prefix.length)) : text;
  const parsed = migratePreset(JSON.parse(payload));
  assertPreset(parsed);
  return parsed;
}

export function shareTextForPreset(preset: BuildPreset): string {
  return `${SHARE_PREFIX}${encodeURIComponent(JSON.stringify(preset))}`;
}

export function downloadPresetJson(preset: BuildPreset) {
  const blob = new Blob([JSON.stringify(preset, null, 2)], { type: "application/json;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${safeFilename(preset.name)}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

export function importBuildPreset(input: BuildPreset | BuildPresetV1): BuildPreset {
  const preset = migratePreset(input);
  assertPreset(preset);
  const index = savedBuildIndex();
  const idConflict = loadBuildPreset(preset.id) !== null;
  const imported = {
    ...preset,
    id: idConflict ? crypto.randomUUID() : preset.id,
    name: uniqueImportedName(preset.name, index.builds.map((entry) => entry.name)),
    createdAt: idConflict ? new Date().toISOString() : preset.createdAt,
    updatedAt: new Date().toISOString(),
  };
  persistPreset(imported);
  return imported;
}

export function replaceImportedBuildPreset(input: BuildPreset | BuildPresetV1): BuildPreset {
  const preset = migratePreset(input);
  assertPreset(preset);
  const imported = { ...preset, updatedAt: new Date().toISOString() };
  persistPreset(imported);
  return imported;
}

export function previewPresetImport(raw: string): PresetImportPreview {
  const preset = parsePresetText(raw);
  const index = savedBuildIndex();
  return {
    preset,
    bytes: new TextEncoder().encode(raw.trim()).byteLength,
    idConflict: loadBuildPreset(preset.id) !== null,
    nameConflict: index.builds.some((entry) => entry.name.toLocaleLowerCase() === preset.name.toLocaleLowerCase()),
  };
}

function persistPreset(preset: BuildPreset) {
  assertPreset(preset);
  const index = savedBuildIndex();
  if (index.builds.length >= 500 && !index.builds.some((entry) => entry.id === preset.id)) {
    throw new Error("Saved build limit reached (500). Delete a build before saving another.");
  }
  const builds = [
    { id: preset.id, name: preset.name, profileId: preset.profileId, dataVersion: preset.dataVersion, updatedAt: preset.updatedAt },
    ...index.builds.filter((entry) => entry.id !== preset.id),
  ];
  const key = `${PRESET_PREFIX}${preset.id}`;
  const previous = localStorage.getItem(key);
  localStorage.setItem(key, JSON.stringify(preset));
  try {
    localStorage.setItem(INDEX_KEY, JSON.stringify({ version: 1, builds } satisfies SavedBuildIndexV1));
  } catch (error) {
    if (previous === null) localStorage.removeItem(key);
    else localStorage.setItem(key, previous);
    throw error;
  }
}

function assertPreset(value: BuildPreset) {
  if (!isRecord(value) || value.version !== 2) throw invalidPreset("schema version must be 2");
  assertText(value.id, "id", 128);
  assertText(value.name, "name", 200);
  assertProfileId(value.profileId, "profileId");
  assertDataVersion(value.dataVersion);
  assertDate(value.createdAt, "createdAt");
  assertDate(value.updatedAt, "updatedAt");
  assertRequest(value.request);
  if (value.profileId !== value.request.profileId) {
    throw invalidPreset("profileId must match request.profileId");
  }
  assertSolvedBuild(value.selectedBuild, "selectedBuild");
  assertSolvedBuild(value.compareTarget, "compareTarget");
  assertArray(value.compareBench, "compareBench", 8);
  value.compareBench.forEach((build, index) => assertSolvedBuild(build, `compareBench[${index}]`));
}

function assertRequest(value: unknown): asserts value is OptimizeRequestDto {
  if (!isRecord(value)) throw invalidPreset("request must be an object");
  assertProfileId(value.profileId, "request.profileId");
  assertText(value.className, "request.className", 80);
  assertInteger(value.characterLevel, "request.characterLevel", 1, 792);
  for (const key of ["vig", "mnd", "end", "strStat", "dex", "intStat", "fai", "arc"] as const) {
    assertInteger(value[key], `request.${key}`, 0, 99);
  }
  for (const key of ["minStr", "minDex", "minInt", "minFai", "minArc"] as const) {
    assertInteger(value[key], `request.${key}`, 0, 99);
  }
  for (const key of ["lockStr", "lockDex", "lockInt", "lockFai", "lockArc"] as const) {
    assertNullableInteger(value[key], `request.${key}`, 0, 99);
  }
  assertInteger(value.standardMaxUpgrade, "request.standardMaxUpgrade", 0, 25);
  // Profile rules are applied when a preset is loaded. Keep the persisted
  // shape wide enough for profiles such as Convergence, whose Somber cap is 15.
  assertInteger(value.somberMaxUpgrade, "request.somberMaxUpgrade", 0, 25);
  assertBoolean(value.exactUpgrade, "request.exactUpgrade");
  assertBoolean(value.twoHanding, "request.twoHanding");
  assertBoolean(value.dlcScaling, "request.dlcScaling");
  assertInteger(value.scadutreeLevel, "request.scadutreeLevel", 0, 20);
  assertNullableText(value.weaponName, "request.weaponName", 200);
  assertNullableText(value.affinity, "request.affinity", 80);
  assertNullableText(value.aowName, "request.aowName", 200);
  assertNullableText(value.weaponTypeKey, "request.weaponTypeKey", 100);
  if (!["all", "standard_only", "somber_only"].includes(String(value.somberFilter))) {
    throw invalidPreset("request.somberFilter is invalid");
  }
  if (!isRecord(value.filters) || value.filters.version !== 1) throw invalidPreset("request.filters must use schema version 1");
  assertArray(value.filters.entries, "request.filters.entries", 512);
  value.filters.entries.forEach((entry, index) => {
    if (!isRecord(entry)) throw invalidPreset(`request.filters.entries[${index}] must be an object`);
    if (!["weapon_family", "weapon_type", "affinity", "aow", "reinforcement", "coverage"].includes(String(entry.dimension))) {
      throw invalidPreset(`request.filters.entries[${index}].dimension is invalid`);
    }
    assertText(entry.id, `request.filters.entries[${index}].id`, 128);
    if (!["include", "exclude"].includes(String(entry.mode))) throw invalidPreset(`request.filters.entries[${index}].mode is invalid`);
  });
  if (!["automatic", "weapon", "loadout"].includes(String(value.resultGrouping))) {
    throw invalidPreset("request.resultGrouping is invalid");
  }
  if (!["max_ar", "max_physical_ar", "max_ar_plus_bleed", "aow_first_hit", "aow_full_sequence"].includes(String(value.objective))) {
    throw invalidPreset("request.objective is invalid");
  }
  assertInteger(value.topK, "request.topK", 1, 500);
}

function assertSolvedBuild(value: unknown, label: string): asserts value is SolvedBuildDto | null {
  if (value === null) return;
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object or null`);
  assertInteger(value.weaponId, `${label}.weaponId`, 0, 0xffff_ffff);
  assertText(value.weaponName, `${label}.weaponName`, 200);
  assertText(value.affinity, `${label}.affinity`, 80);
  assertBoolean(value.isSomber, `${label}.isSomber`);
  assertInteger(value.upgrade, `${label}.upgrade`, 0, 25);
  assertCombatStats(value.stats, `${label}.stats`);
  assertDamage(value.ar, `${label}.ar`);
  assertNullableInteger(value.aowId, `${label}.aowId`, 0, 0xffff);
  assertNullableText(value.aowName, `${label}.aowName`, 200);
  for (const key of [
    "bleedBuildup", "bleedBuildupAdd", "frostBuildup", "poisonBuildup",
    "scarletRotBuildup", "sleepBuildup", "madnessBuildup", "deathBuildup",
    "aowFirstHitDamage", "aowFullSequenceDamage", "score",
  ] as const) assertFinite(value[key], `${label}.${key}`);
  assertAowRoute(value.aowRoute, `${label}.aowRoute`);
}

function assertAowRoute(value: unknown, label: string) {
  if (value === null) return;
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object or null`);
  assertText(value.routeId, `${label}.routeId`, 200);
  assertText(value.routeLabel, `${label}.routeLabel`, 300);
  assertInteger(value.routePriority, `${label}.routePriority`, -1_000_000, 1_000_000);
  assertNullableText(value.buffActivationActionId, `${label}.buffActivationActionId`, 200);
  assertFinite(value.firstHitDamage, `${label}.firstHitDamage`);
  assertDamage(value.totalDamage, `${label}.totalDamage`);
  assertStatus(value.totalStatusBuildup, `${label}.totalStatusBuildup`);
  assertFinite(value.totalStaminaCost, `${label}.totalStaminaCost`);
  assertArray(value.actions, `${label}.actions`, 512);
  value.actions.forEach((action, actionIndex) => {
    const actionLabel = `${label}.actions[${actionIndex}]`;
    if (!isRecord(action)) throw invalidPreset(`${actionLabel} must be an object`);
    assertText(action.actionId, `${actionLabel}.actionId`, 200);
    assertInteger(action.actionOrder, `${actionLabel}.actionOrder`, 0, 1_000_000);
    assertFinite(action.staminaCost, `${actionLabel}.staminaCost`);
    assertArray(action.hits, `${actionLabel}.hits`, 4096);
    action.hits.forEach((hit, hitIndex) => assertAowHit(hit, `${actionLabel}.hits[${hitIndex}]`));
  });
}

function assertAowHit(value: unknown, label: string) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  assertInteger(value.sheetRow, `${label}.sheetRow`, 0, 10_000_000);
  assertInteger(value.hitOrder, `${label}.hitOrder`, 0, 1_000_000);
  assertText(value.rawName, `${label}.rawName`, 500, true);
  assertDamage(value.damage, `${label}.damage`);
  assertStatus(value.statusBuildup, `${label}.statusBuildup`);
  assertText(value.physicalAttackAttribute, `${label}.physicalAttackAttribute`, 80, true);
  assertBoolean(value.buffActive, `${label}.buffActive`);
  assertArray(value.warnings, `${label}.warnings`, 256);
  value.warnings.forEach((warning, index) => assertText(warning, `${label}.warnings[${index}]`, 1000, true));
  assertArray(value.effects, `${label}.effects`, 256);
  value.effects.forEach((effect, index) => {
    const effectLabel = `${label}.effects[${index}]`;
    if (!isRecord(effect)) throw invalidPreset(`${effectLabel} must be an object`);
    assertInteger(effect.effectId, `${effectLabel}.effectId`, 0, 0xffff_ffff);
    assertText(effect.effectName, `${effectLabel}.effectName`, 500, true);
    assertText(effect.role, `${effectLabel}.role`, 100);
    assertText(effect.activationTiming, `${effectLabel}.activationTiming`, 100);
    assertBoolean(effect.isSupported, `${effectLabel}.isSupported`);
    assertText(effect.reason, `${effectLabel}.reason`, 1000, true);
    assertDamage(effect.attackPower, `${effectLabel}.attackPower`);
    assertStatus(effect.statusBuildup, `${effectLabel}.statusBuildup`);
  });
}

function assertCombatStats(value: unknown, label: string) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  for (const key of ["strStat", "dex", "intStat", "fai", "arc"] as const) {
    assertInteger(value[key], `${label}.${key}`, 0, 99);
  }
}

function assertDamage(value: unknown, label: string) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  for (const key of ["physical", "magic", "fire", "lightning", "holy", "total"] as const) {
    assertFinite(value[key], `${label}.${key}`);
  }
}

function assertStatus(value: unknown, label: string) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  for (const key of ["bleed", "frost", "poison", "scarletRot", "sleep", "madness", "death"] as const) {
    assertFinite(value[key], `${label}.${key}`);
  }
}

function assertDataVersion(value: unknown) {
  assertText(value, "dataVersion", 300);
  const parts = value.split(":");
  if (![3, 4].includes(parts.length) || parts.some((part) => !part.trim())) {
    throw invalidPreset("dataVersion must contain profile (when available), schema, dataset, and model identifiers");
  }
}

function assertProfileId(value: unknown, label: string): asserts value is string {
  assertText(value, label, 80);
  if (!/^[a-z0-9][a-z0-9_-]*$/.test(value)) throw invalidPreset(`${label} is invalid`);
}

function migratePreset(value: unknown): BuildPreset {
  if (!isRecord(value) || ![1, 2].includes(Number(value.version))) {
    throw invalidPreset("schema version must be 1 or 2");
  }
  const request = value.request;
  if (!isRecord(request)) throw invalidPreset("request must be an object");
  delete request.budgetMode;
  delete request.offensivePointBudget;
  const legacy = value.version === 1;
  if (!legacy) return value as unknown as BuildPreset;
  if (typeof value.profileId !== "string" && typeof request.profileId !== "string") {
    value.profileId = "vanilla";
    request.profileId = "vanilla";
  } else if (typeof value.profileId !== "string") {
    value.profileId = request.profileId;
  } else if (typeof request.profileId !== "string") {
    request.profileId = value.profileId;
  }
  if (request.characterLevel !== undefined) {
    assertInteger(request.characterLevel, "request.characterLevel", 1, 792);
  }
  const classInfo = STARTING_CLASS_METADATA.find((entry) => entry.name === request.className);
  if (classInfo) {
    const keys = ["vig", "mnd", "end", "strStat", "dex", "intStat", "fai", "arc"];
    const total = keys.reduce((sum, key) => sum + (typeof request[key] === "number" ? Number(request[key]) : 0), 0);
    request.characterLevel = Math.max(classInfo.baseLevel, Math.min(713, classInfo.baseLevel + total - classInfo.baseTotal));
  }
  request.filters ??= { version: 1, entries: [] };
  request.resultGrouping ??= "automatic";
  for (const buildKey of ["selectedBuild", "compareTarget"] as const) {
    const build = value[buildKey];
    if (!isRecord(build)) continue;
    for (const statusKey of ["sleepBuildup", "madnessBuildup", "deathBuildup"] as const) {
      if (build[statusKey] === undefined) build[statusKey] = 0;
    }
  }
  if (!Array.isArray(value.compareBench)) {
    value.compareBench = value.compareTarget ? [value.compareTarget] : [];
  }
  value.version = 2;
  return value as unknown as BuildPreset;
}

function assertDate(value: unknown, label: string) {
  assertText(value, label, 64);
  if (Number.isNaN(Date.parse(value))) throw invalidPreset(`${label} must be an ISO date`);
}

function assertText(value: unknown, label: string, maxLength: number, allowEmpty = false): asserts value is string {
  if (typeof value !== "string" || value.length > maxLength || (!allowEmpty && !value.trim())) {
    throw invalidPreset(`${label} must be ${allowEmpty ? "a" : "a non-empty"} string no longer than ${maxLength} characters`);
  }
}

function assertNullableText(value: unknown, label: string, maxLength: number): asserts value is string | null {
  if (value !== null) assertText(value, label, maxLength);
}

function assertFinite(value: unknown, label: string): asserts value is number {
  if (typeof value !== "number" || !Number.isFinite(value)) throw invalidPreset(`${label} must be finite`);
}

function assertInteger(value: unknown, label: string, min: number, max: number): asserts value is number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
    throw invalidPreset(`${label} must be an integer from ${min} through ${max}`);
  }
}

function assertNullableInteger(value: unknown, label: string, min: number, max: number): asserts value is number | null {
  if (value !== null) assertInteger(value, label, min, max);
}

function assertBoolean(value: unknown, label: string): asserts value is boolean {
  if (typeof value !== "boolean") throw invalidPreset(`${label} must be true or false`);
}

function assertArray(value: unknown, label: string, maxLength: number): asserts value is unknown[] {
  if (!Array.isArray(value) || value.length > maxLength) throw invalidPreset(`${label} must be an array with at most ${maxLength} entries`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function invalidPreset(detail: string): Error {
  return new Error(`Import text is not a valid BuildPreset preset: ${detail}.`);
}

function uniqueImportedName(name: string, existingNames: string[]): string {
  const occupied = new Set(existingNames.map((entry) => entry.toLocaleLowerCase()));
  if (!occupied.has(name.toLocaleLowerCase())) return name;
  for (let number = 1; ; number += 1) {
    const suffix = number === 1 ? " (imported)" : ` (imported ${number})`;
    const candidate = `${name.slice(0, 200 - suffix.length).trimEnd()}${suffix}`;
    if (!occupied.has(candidate.toLocaleLowerCase())) return candidate;
  }
}

function readJson<T>(key: string): T | null {
  const raw = localStorage.getItem(key);
  if (!raw) {
    return null;
  }
  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function safeFilename(value: string): string {
  return value.trim().replace(/[^a-z0-9_-]+/gi, "-").replace(/^-|-$/g, "") || "build-preset";
}
