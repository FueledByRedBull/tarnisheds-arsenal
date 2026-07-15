import { BuildPresetV1, OptimizeRequestDto, SavedBuildIndexV1, SolvedBuildDto } from "./types";

const INDEX_KEY = "tarnisheds-arsenal.savedBuildIndex.v1";
const PRESET_PREFIX = "tarnisheds-arsenal.savedBuild.v1.";
export const SHARE_PREFIX = "ta-v1:";
export const MAX_PRESET_IMPORT_BYTES = 256 * 1024;

export interface PresetImportPreview {
  preset: BuildPresetV1;
  bytes: number;
  idConflict: boolean;
  nameConflict: boolean;
}

export function savedBuildIndex(): SavedBuildIndexV1 {
  const value = readJson<unknown>(INDEX_KEY);
  if (!isRecord(value) || value.version !== 1 || !Array.isArray(value.builds) || value.builds.length > 500) {
    return { version: 1, builds: [] };
  }
  const builds = value.builds.filter((entry): entry is SavedBuildIndexV1["builds"][number] => {
    if (!isRecord(entry)) return false;
    try {
      assertText(entry.id, "saved index id", 128);
      assertText(entry.name, "saved index name", 200);
      assertDataVersion(entry.dataVersion);
      assertDate(entry.updatedAt, "saved index updatedAt");
      return true;
    } catch {
      return false;
    }
  });
  return { version: 1, builds };
}

export function loadBuildPreset(id: string): BuildPresetV1 | null {
  const preset = readJson<unknown>(`${PRESET_PREFIX}${id}`);
  try {
    assertPreset(preset as BuildPresetV1);
    return preset as BuildPresetV1;
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
  dataVersion: string;
}): BuildPresetV1 {
  const now = new Date().toISOString();
  const existing = input.id ? loadBuildPreset(input.id) : null;
  const preset: BuildPresetV1 = {
    version: 1,
    id: input.id ?? crypto.randomUUID(),
    name: input.name.trim() || "Untitled Build",
    request: input.request,
    selectedBuild: input.selectedBuild,
    compareTarget: input.compareTarget,
    dataVersion: input.dataVersion,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };
  assertPreset(preset);
  localStorage.setItem(`${PRESET_PREFIX}${preset.id}`, JSON.stringify(preset));
  upsertIndex(preset);
  return preset;
}

export function renameBuildPreset(id: string, name: string): BuildPresetV1 {
  const preset = loadBuildPreset(id);
  if (!preset) {
    throw new Error("Saved build was not found.");
  }
  const renamed = { ...preset, name: name.trim() || preset.name, updatedAt: new Date().toISOString() };
  localStorage.setItem(`${PRESET_PREFIX}${id}`, JSON.stringify(renamed));
  upsertIndex(renamed);
  return renamed;
}

export function deleteBuildPreset(id: string) {
  localStorage.removeItem(`${PRESET_PREFIX}${id}`);
  const index = savedBuildIndex();
  localStorage.setItem(INDEX_KEY, JSON.stringify({
    version: 1,
    builds: index.builds.filter((entry) => entry.id !== id),
  } satisfies SavedBuildIndexV1));
}

export function parsePresetText(raw: string): BuildPresetV1 {
  const text = raw.trim();
  const bytes = new TextEncoder().encode(text).byteLength;
  if (bytes > MAX_PRESET_IMPORT_BYTES) {
    throw new Error(`Preset is too large (${bytes.toLocaleString()} bytes; limit ${MAX_PRESET_IMPORT_BYTES.toLocaleString()}).`);
  }
  const payload = text.startsWith(SHARE_PREFIX)
    ? decodeURIComponent(text.slice(SHARE_PREFIX.length))
    : text;
  const parsed = JSON.parse(payload) as BuildPresetV1;
  assertPreset(parsed);
  return parsed;
}

export function shareTextForPreset(preset: BuildPresetV1): string {
  return `${SHARE_PREFIX}${encodeURIComponent(JSON.stringify(preset))}`;
}

export function downloadPresetJson(preset: BuildPresetV1) {
  const blob = new Blob([JSON.stringify(preset, null, 2)], { type: "application/json;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${safeFilename(preset.name)}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

export function importBuildPreset(preset: BuildPresetV1): BuildPresetV1 {
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
  localStorage.setItem(`${PRESET_PREFIX}${imported.id}`, JSON.stringify(imported));
  upsertIndex(imported);
  return imported;
}

export function replaceImportedBuildPreset(preset: BuildPresetV1): BuildPresetV1 {
  assertPreset(preset);
  const imported = { ...preset, updatedAt: new Date().toISOString() };
  localStorage.setItem(`${PRESET_PREFIX}${imported.id}`, JSON.stringify(imported));
  upsertIndex(imported);
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

function upsertIndex(preset: BuildPresetV1) {
  const index = savedBuildIndex();
  const builds = [
    { id: preset.id, name: preset.name, dataVersion: preset.dataVersion, updatedAt: preset.updatedAt },
    ...index.builds.filter((entry) => entry.id !== preset.id),
  ];
  localStorage.setItem(INDEX_KEY, JSON.stringify({ version: 1, builds } satisfies SavedBuildIndexV1));
}

function assertPreset(value: BuildPresetV1) {
  if (!isRecord(value) || value.version !== 1) throw invalidPreset("schema version must be 1");
  assertText(value.id, "id", 128);
  assertText(value.name, "name", 200);
  assertDataVersion(value.dataVersion);
  assertDate(value.createdAt, "createdAt");
  assertDate(value.updatedAt, "updatedAt");
  assertRequest(value.request);
  assertSolvedBuild(value.selectedBuild, "selectedBuild");
  assertSolvedBuild(value.compareTarget, "compareTarget");
}

function assertRequest(value: unknown): asserts value is OptimizeRequestDto {
  if (!isRecord(value)) throw invalidPreset("request must be an object");
  assertText(value.className, "request.className", 80);
  assertInteger(value.characterLevel, "request.characterLevel", 1, 713);
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
  assertInteger(value.somberMaxUpgrade, "request.somberMaxUpgrade", 0, 10);
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
    "scarletRotBuildup", "aowFirstHitDamage", "aowFullSequenceDamage", "score",
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
  if (parts.length !== 3 || parts.some((part) => !part.trim())) {
    throw invalidPreset("dataVersion must contain schema, dataset, and model identifiers");
  }
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
  return new Error(`Import text is not a valid BuildPresetV1 preset: ${detail}.`);
}

function uniqueImportedName(name: string, existingNames: string[]): string {
  const occupied = new Set(existingNames.map((entry) => entry.toLocaleLowerCase()));
  if (!occupied.has(name.toLocaleLowerCase())) return name;
  if (!occupied.has(`${name} (imported)`.toLocaleLowerCase())) return `${name} (imported)`;
  let suffix = 2;
  while (occupied.has(`${name} (imported ${suffix})`.toLocaleLowerCase())) suffix += 1;
  return `${name} (imported ${suffix})`;
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
