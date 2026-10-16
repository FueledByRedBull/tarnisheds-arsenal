import { MAX_COMPARE_PINS } from "./compare-bench";
import { downloadText } from "./csv";
import { errorMessage } from "./format";
import {
  assertArray, assertBoolean, assertInteger, assertNullableInteger, assertNullableText, assertText, invalidPreset, isRecord, validateStoredBuild,
} from "./stored-build";
import { STARTING_CLASS_METADATA } from "./session";
import {
  BuildPreset, BuildPresetV1, FILTER_DIMENSION_IDS, OBJECTIVE_IDS, OptimizeRequestDto, SavedBuildIndexV1, SolvedBuildDto,
} from "./types";

const INDEX_KEY = "tarnisheds-arsenal.savedBuildIndex.v1";
const PRESET_PREFIX = "tarnisheds-arsenal.savedBuild.v2.";
const LEGACY_PRESET_PREFIX = "tarnisheds-arsenal.savedBuild.v1.";
const RECOVERY_PREFIX = "tarnisheds-arsenal.savedBuildIndex.recovery.";
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
  const raw = localStorage.getItem(INDEX_KEY);
  if (raw === null) {
    if (storageKeys(PRESET_PREFIX, LEGACY_PRESET_PREFIX).length) throw new Error("Saved build index is missing. Open backup and recovery before saving.");
    return { version: 1, builds: [] };
  }
  try {
    const value: unknown = JSON.parse(raw);
    if (!isRecord(value) || value.version !== 1 || !Array.isArray(value.builds)) throw new Error();
    const ids = new Set<string>();
    for (const entry of value.builds) {
      if (!isRecord(entry)) throw new Error();
      assertText(entry.id, "saved index id", 128);
      assertText(entry.name, "saved index name", 200);
      entry.profileId ??= "vanilla";
      assertProfileId(entry.profileId, "saved index profileId");
      assertDataVersion(entry.dataVersion);
      assertDate(entry.updatedAt, "saved index updatedAt");
      if (ids.has(entry.id)) throw new Error();
      ids.add(entry.id);
    }
    return value as unknown as SavedBuildIndexV1;
  } catch {
    throw new Error("Saved build index is damaged. Open backup and recovery before saving; original data is preserved.");
  }
}

export const MAX_BUILD_BACKUP_BYTES = 10 * 1024 * 1024;
export const MAX_BUILD_BACKUP_COUNT = 500;

export interface SavedBuildInspection {
  presets: BuildPreset[];
  issues: string[];
  orphanCount: number;
  needsRecovery: boolean;
  // Exact source bytes let confirmation reject changes made since the preview.
  source: { index: string | null; records: [string, string | null][] };
}

function storageKeys(...prefixes: string[]): string[] {
  const keys: string[] = [];
  for (let index = 0; index < localStorage.length; index += 1) {
    const key = localStorage.key(index);
    if (key !== null && prefixes.some((prefix) => key.startsWith(prefix))) keys.push(key);
  }
  return keys.sort();
}

function presetIndexEntry(preset: BuildPreset): SavedBuildIndexV1["builds"][number] {
  const { id, name, profileId, dataVersion, updatedAt } = preset;
  return { id, name, profileId, dataVersion, updatedAt };
}

export function inspectSavedBuilds(): SavedBuildInspection {
  const source = {
    index: localStorage.getItem(INDEX_KEY),
    records: storageKeys(PRESET_PREFIX, LEGACY_PRESET_PREFIX).map((key): [string, string | null] => [key, localStorage.getItem(key)]),
  };
  const issues: string[] = [];
  let index: SavedBuildIndexV1 = { version: 1, builds: [] };
  let needsRecovery = false;
  try { index = savedBuildIndex(); }
  catch (error) { issues.push(String((error as Error).message)); needsRecovery = true; }
  const valid = new Map<string, BuildPreset>();
  // Prefer the current record when both schema versions remain after migration.
  for (const [key, raw] of source.records) {
    try {
      const preset = readPreset(JSON.parse(raw ?? ""));
      const prefix = key.startsWith(PRESET_PREFIX) ? PRESET_PREFIX : LEGACY_PRESET_PREFIX;
      if (preset.id !== key.slice(prefix.length)) throw new Error("record ID does not match its storage key");
      valid.set(preset.id, preset);
    } catch (error) { issues.push(`${key}: ${(error as Error).message}`); }
  }
  const indexedIds = new Set(index.builds.map((entry) => entry.id));
  for (const entry of index.builds) {
    const preset = valid.get(entry.id);
    if (!preset) { issues.push(`Saved build '${entry.name}' has no readable record.`); needsRecovery = true; }
    else if (Object.entries(presetIndexEntry(preset)).some(([key, value]) => entry[key as keyof typeof entry] !== value)) {
      issues.push(`Saved build '${entry.name}' has out-of-date index details.`);
      needsRecovery = true;
    }
  }
  const presets = [...valid.values()].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id));
  const orphanCount = presets.filter((preset) => !indexedIds.has(preset.id)).length;
  return { presets, issues, orphanCount, needsRecovery: needsRecovery || orphanCount > 0, source };
}

export function recoverSavedBuilds(preview: SavedBuildInspection): string | null {
  const current = inspectSavedBuilds();
  if (JSON.stringify(current.source) !== JSON.stringify(preview.source)) {
    throw new Error("Saved builds changed after this preview. Scan again before recovery.");
  }
  const backupKey = current.source.index === null ? null : `${RECOVERY_PREFIX}${new Date().toISOString()}`;
  if (backupKey) localStorage.setItem(backupKey, current.source.index!);
  // Copy the original index first; a quota failure must never erase its only copy.
  localStorage.setItem(INDEX_KEY, JSON.stringify({ version: 1, builds: current.presets.map(presetIndexEntry) }));
  // Keep the newest three copies: each takes up to ~100 KB of the quota the builds share.
  // Older copies named by a random ID sort first, as the oldest.
  const copies = storageKeys(RECOVERY_PREFIX)
    .sort((a, b) => recoveryOrder(a).localeCompare(recoveryOrder(b)));
  for (const key of copies.slice(0, -3)) localStorage.removeItem(key);
  return backupKey;
}

function recoveryOrder(key: string): string {
  const name = key.slice(RECOVERY_PREFIX.length);
  return /^\d{4}-/.test(name) ? name : "";
}

// Export never refuses: it is how a recovered library over the import limits gets off the
// device, even though a restore then needs it split into backups of at most 500 builds.
export function savedBuildBackupText(inspection: SavedBuildInspection): string {
  return JSON.stringify({ format: "tarnisheds-arsenal.build-backup", version: 1, builds: inspection.presets });
}

export function previewBuildBackup(raw: string): { presets: BuildPreset[]; bytes: number } {
  // UTF-16 length never exceeds UTF-8 bytes, so oversized text is rejected before encoding.
  const bytes = raw.length > MAX_BUILD_BACKUP_BYTES ? raw.length : utf8Bytes(raw);
  if (bytes > MAX_BUILD_BACKUP_BYTES) throw new Error("Build backup is too large (limit 10 MiB).");
  const parsed: unknown = JSON.parse(raw);
  if (!isRecord(parsed) || parsed.format !== "tarnisheds-arsenal.build-backup" || parsed.version !== 1) {
    throw new Error("Not a supported saved-build backup. Individual presets use Import JSON or Share Text.");
  }
  assertArray(parsed.builds, "backup builds", MAX_BUILD_BACKUP_COUNT);
  const presets = parsed.builds.map(importedPreset);
  const ids = new Set(presets.map((preset) => preset.id));
  if (ids.size !== presets.length) throw new Error("Build backup contains duplicate IDs.");
  return { presets, bytes };
}

export function restoreBuildBackup(raw: string): BuildPreset[] {
  const { presets } = previewBuildBackup(raw);
  const inspection = inspectSavedBuilds();
  if (inspection.needsRecovery) throw new Error("Recover the saved-build index before restoring a backup.");
  const index = savedBuildIndex();
  if (index.builds.length + presets.length > MAX_BUILD_BACKUP_COUNT) throw new Error("Saved build limit reached (500). Export or delete builds before restoring.");
  const names = index.builds.map((entry) => entry.name);
  const now = new Date().toISOString();
  const imported = presets.map((preset) => {
    const name = uniqueImportedName(preset.name, names);
    names.push(name);
    // Restores never overwrite existing records, including unreadable records.
    let id: string;
    do { id = crypto.randomUUID(); } while (localStorage.getItem(`${PRESET_PREFIX}${id}`) !== null || localStorage.getItem(`${LEGACY_PRESET_PREFIX}${id}`) !== null);
    return { ...preset, id, name, updatedAt: now };
  });
  const written: string[] = [];
  try {
    for (const preset of imported) {
      const key = `${PRESET_PREFIX}${preset.id}`;
      localStorage.setItem(key, JSON.stringify(preset));
      written.push(key);
    }
    localStorage.setItem(INDEX_KEY, JSON.stringify({ version: 1, builds: [...imported.map(presetIndexEntry), ...index.builds] }));
  } catch (error) {
    // If the process stops before index commit, these new records are recoverable orphans.
    for (const key of written) {
      try { localStorage.removeItem(key); } catch { /* Leave the recoverable record when storage is unavailable. */ }
    }
    throw error;
  }
  return imported;
}

export function loadBuildPreset(id: string): BuildPreset | null {
  for (const prefix of [PRESET_PREFIX, LEGACY_PRESET_PREFIX]) {
    const raw = localStorage.getItem(`${prefix}${id}`);
    if (raw === null) continue;
    try {
      const preset = readPreset(JSON.parse(raw));
      if (preset.id === id) return preset;
    } catch { /* A readable older record can still be recovered. */ }
  }
  return null;
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

// The limit applies to the preset's compact JSON, the form saves store, so every saved build
// also imports from its share text (which percent-encoding makes up to three times longer).
export function parsePresetText(raw: string): BuildPreset {
  const text = raw.trim();
  if (text.length > 3 * MAX_PRESET_IMPORT_BYTES) throw presetTooLarge();
  const prefix = text.startsWith(SHARE_PREFIX) ? SHARE_PREFIX : text.startsWith(LEGACY_SHARE_PREFIX) ? LEGACY_SHARE_PREFIX : "";
  let payload = text;
  if (prefix) {
    try {
      payload = decodeURIComponent(text.slice(prefix.length));
    } catch {
      throw new Error("Share text is damaged or incomplete. Copy it again from its source.");
    }
  }
  const preset = importedPreset(JSON.parse(payload));
  assertPresetSize(JSON.stringify(preset));
  return preset;
}

export function shareTextForPreset(preset: BuildPreset): string {
  return `${SHARE_PREFIX}${encodeURIComponent(JSON.stringify(preset))}`;
}

export function downloadPresetJson(preset: BuildPreset) {
  downloadText(`${safeFilename(preset.name)}.json`, JSON.stringify(preset, null, 2), "application/json;charset=utf-8");
}

export function importBuildPreset(input: BuildPreset | BuildPresetV1): BuildPreset {
  const preset = readPreset(input);
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
  const preset = readPreset(input);
  const imported = { ...preset, updatedAt: new Date().toISOString() };
  persistPreset(imported);
  return imported;
}

export function previewPresetImport(raw: string): PresetImportPreview {
  const preset = parsePresetText(raw);
  const index = savedBuildIndex();
  return {
    preset,
    bytes: utf8Bytes(JSON.stringify(preset)),
    idConflict: loadBuildPreset(preset.id) !== null,
    nameConflict: index.builds.some((entry) => entry.name.toLocaleLowerCase() === preset.name.toLocaleLowerCase()),
  };
}

function persistPreset(preset: BuildPreset) {
  assertPreset(preset);
  const text = JSON.stringify(preset);
  assertPresetSize(text);
  const index = savedBuildIndex();
  if (index.builds.length >= MAX_BUILD_BACKUP_COUNT && !index.builds.some((entry) => entry.id === preset.id)) {
    throw new Error("Saved build limit reached (500). Delete a build before saving another.");
  }
  const builds = [presetIndexEntry(preset), ...index.builds.filter((entry) => entry.id !== preset.id)];
  const key = `${PRESET_PREFIX}${preset.id}`;
  const previous = localStorage.getItem(key);
  if (previous !== null) {
    try {
      if (readPreset(JSON.parse(previous)).id !== preset.id) throw new Error();
    } catch {
      throw new Error("Existing saved build is unreadable. Export available builds and use a new ID; original data is preserved.");
    }
  }
  try {
    localStorage.setItem(key, text);
    try {
      localStorage.setItem(INDEX_KEY, JSON.stringify({ version: 1, builds } satisfies SavedBuildIndexV1));
    } catch (error) {
      if (previous === null) localStorage.removeItem(key);
      else localStorage.setItem(key, previous);
      throw error;
    }
  } catch (error) {
    if (error instanceof DOMException && error.name === "QuotaExceededError") {
      throw new Error("Device storage for saved builds is full. Export a backup, then delete builds you no longer need; nothing was changed.");
    }
    throw error;
  }
}

function readPreset(value: unknown): BuildPreset {
  const preset = migratePreset(value);
  assertPreset(preset);
  return preset;
}

function importedPreset(value: unknown): BuildPreset {
  try {
    return readPreset(value);
  } catch (error) {
    throw new Error(`Import text is not a valid BuildPreset preset: ${errorMessage(error)}.`);
  }
}

function utf8Bytes(text: string): number {
  return new TextEncoder().encode(text).byteLength;
}

function presetTooLarge(): Error {
  return new Error(`Preset is too large (limit ${MAX_PRESET_IMPORT_BYTES / 1024} KiB of JSON). Pin fewer comparison builds before saving or sharing it.`);
}

function assertPresetSize(json: string) {
  if (utf8Bytes(json) > MAX_PRESET_IMPORT_BYTES) throw presetTooLarge();
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
  assertArray(value.compareBench, "compareBench", MAX_COMPARE_PINS);
  value.compareBench.forEach((build, index) => {
    if (build === null) throw invalidPreset(`compareBench[${index}] must be a build`);
    assertSolvedBuild(build, `compareBench[${index}]`);
  });
}

export function assertRequest(value: unknown): asserts value is OptimizeRequestDto {
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
  if (typeof value.somberFilter !== "string" || !["all", "standard_only", "somber_only"].includes(value.somberFilter)) {
    throw invalidPreset("request.somberFilter is invalid");
  }
  if (!isRecord(value.filters) || value.filters.version !== 1) throw invalidPreset("request.filters must use schema version 1");
  assertArray(value.filters.entries, "request.filters.entries", 512);
  value.filters.entries.forEach((entry, index) => {
    if (!isRecord(entry)) throw invalidPreset(`request.filters.entries[${index}] must be an object`);
    if (typeof entry.dimension !== "string" || !(FILTER_DIMENSION_IDS as readonly string[]).includes(entry.dimension)) {
      throw invalidPreset(`request.filters.entries[${index}].dimension is invalid`);
    }
    assertText(entry.id, `request.filters.entries[${index}].id`, 128);
    if (typeof entry.mode !== "string" || !["include", "exclude"].includes(entry.mode)) throw invalidPreset(`request.filters.entries[${index}].mode is invalid`);
  });
  if (typeof value.resultGrouping !== "string" || !["automatic", "weapon", "loadout"].includes(value.resultGrouping)) {
    throw invalidPreset("request.resultGrouping is invalid");
  }
  if (typeof value.objective !== "string" || !(OBJECTIVE_IDS as readonly string[]).includes(value.objective)) {
    throw invalidPreset("request.objective is invalid");
  }
  assertInteger(value.topK, "request.topK", 1, 500);
}

function assertSolvedBuild(value: unknown, label: string): asserts value is SolvedBuildDto | null {
  if (value === null) return;
  const checked = validateStoredBuild(value, { standardMaxUpgrade: 25, somberMaxUpgrade: 25 }, label);
  if ("error" in checked) throw invalidPreset(checked.error);
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
  // Schema-1 records predate later result fields. Loading re-solves saved results, so neutral
  // defaults only let the record be read; they are never shown as verified values.
  const builds = [value.selectedBuild, value.compareTarget, ...(Array.isArray(value.compareBench) ? value.compareBench : [])];
  for (const build of builds) {
    if (!isRecord(build)) continue;
    for (const key of ["bleedBuildupAdd", "frostBuildup", "poisonBuildup", "scarletRotBuildup", "sleepBuildup",
      "madnessBuildup", "deathBuildup", "aowFirstHitDamage", "aowFullSequenceDamage"]) {
      if (build[key] === undefined) build[key] = 0;
    }
    if (build.aowId === undefined) build.aowId = null;
    if (build.aowRoute === undefined) build.aowRoute = null;
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

function uniqueImportedName(name: string, existingNames: string[]): string {
  const occupied = new Set(existingNames.map((entry) => entry.toLocaleLowerCase()));
  if (!occupied.has(name.toLocaleLowerCase())) return name;
  for (let number = 1; ; number += 1) {
    const suffix = number === 1 ? " (imported)" : ` (imported ${number})`;
    const candidate = `${name.slice(0, 200 - suffix.length).trimEnd()}${suffix}`;
    if (!occupied.has(candidate.toLocaleLowerCase())) return candidate;
  }
}

function safeFilename(value: string): string {
  return value.trim().replace(/[^a-z0-9_-]+/gi, "-").replace(/^-|-$/g, "") || "build-preset";
}
