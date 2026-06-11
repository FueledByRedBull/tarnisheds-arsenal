import { BuildPresetV1, OptimizeRequestDto, SavedBuildIndexV1, SolvedBuildDto } from "./types";

const INDEX_KEY = "tarnisheds-arsenal.savedBuildIndex.v1";
const PRESET_PREFIX = "tarnisheds-arsenal.savedBuild.v1.";
export const SHARE_PREFIX = "ta-v1:";

export function savedBuildIndex(): SavedBuildIndexV1 {
  return readJson<SavedBuildIndexV1>(INDEX_KEY) ?? { version: 1, builds: [] };
}

export function loadBuildPreset(id: string): BuildPresetV1 | null {
  return readJson<BuildPresetV1>(`${PRESET_PREFIX}${id}`);
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
  const imported = { ...preset, updatedAt: new Date().toISOString() };
  localStorage.setItem(`${PRESET_PREFIX}${imported.id}`, JSON.stringify(imported));
  upsertIndex(imported);
  return imported;
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
  if (!value || value.version !== 1 || !value.id || !value.request) {
    throw new Error("Import text is not a BuildPresetV1 preset.");
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
