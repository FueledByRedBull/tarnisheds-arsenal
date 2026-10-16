import { isRecord, validateStoredBuild } from "./stored-build";
import type { CatalogDto, Notice, SolvedBuildDto } from "./types";

export const MAX_COMPARE_PINS = 8;

function compareBenchKey(profileId: string): string {
  return `tarnisheds-arsenal.compareBench.v1.${profileId}`;
}

export function readCompareBench(catalog: CatalogDto): { rows: SolvedBuildDto[]; notices: Notice[] } {
  const empty = { rows: [], notices: [] };
  if (typeof localStorage === "undefined") return empty;
  try {
    const raw = localStorage.getItem(compareBenchKey(catalog.dataManifest.profile.id));
    if (!raw) return empty;
    const value: unknown = JSON.parse(raw);
    if (!isRecord(value) || value.version !== 1 || !Array.isArray(value.rows)) return empty;
    if (value.datasetVersion !== catalog.dataManifest.datasetVersion || value.schemaVersion !== catalog.dataManifest.schemaVersion
      || value.modelVersion !== catalog.dataManifest.modelVersion) {
      return { rows: [], notices: value.rows.length ? [{
        scope: "global", tone: "warning", message: "Your pinned comparison builds were saved on different game data and were not restored.",
      }] : [] };
    }
    const rows = value.rows.filter((row): row is SolvedBuildDto => isStoredBuild(row, catalog));
    return {
      rows: rows.slice(0, MAX_COMPARE_PINS),
      notices: rows.length === value.rows.length ? [] : [{
        scope: "global", tone: "warning", message: "Some saved comparison builds were discarded because their data is invalid.",
      }],
    };
  } catch {
    return empty;
  }
}

function isStoredBuild(build: unknown, catalog: CatalogDto): build is SolvedBuildDto {
  return "value" in validateStoredBuild(build, catalog.dataManifest.rules);
}

export function writeCompareBench(catalog: CatalogDto | null, rows: SolvedBuildDto[]): Notice[] {
  if (!catalog) return [];
  try {
    if (typeof localStorage === "undefined") return [];
    localStorage.setItem(compareBenchKey(catalog.dataManifest.profile.id), JSON.stringify({
      version: 1,
      datasetVersion: catalog.dataManifest.datasetVersion,
      schemaVersion: catalog.dataManifest.schemaVersion,
      modelVersion: catalog.dataManifest.modelVersion,
      rows,
    }));
    return [];
  } catch {
    return [{ scope: "global", tone: "warning", message: "Comparison changes could not be saved to device storage and may be lost after restarting. Your saved builds are unchanged." }];
  }
}
