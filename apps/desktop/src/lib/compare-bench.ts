import { validateStoredBuild } from "./stored-build";
import type { CatalogDto, Notice, SolvedBuildDto } from "./types";

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
    if (!isRecord(value) || value.version !== 1 || value.datasetVersion !== catalog.dataManifest.datasetVersion
      || value.schemaVersion !== catalog.dataManifest.schemaVersion
      || value.modelVersion !== catalog.dataManifest.modelVersion || !Array.isArray(value.rows)) {
      return empty;
    }
    const rows = value.rows.filter((row): row is SolvedBuildDto => isStoredBuild(row, catalog));
    return {
      rows: rows.slice(0, 8),
      notices: rows.length === value.rows.length ? [] : [{
        scope: "global", tone: "warning", message: "Some saved comparison builds were discarded because their data is invalid.",
      }],
    };
  } catch {
    return empty;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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
