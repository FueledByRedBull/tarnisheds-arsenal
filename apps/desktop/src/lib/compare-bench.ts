import { isRecord, validateStoredBuild } from "./stored-build";
import type { CatalogDto, Notice, SolvedBuildDto } from "./types";

export const MAX_COMPARE_PINS = 8;

function compareBenchKey(profileId: string): string {
  return `tarnisheds-arsenal.compareBench.v1.${profileId}`;
}

/** Pins saved on other game data come back as `staleRows`, to be recalculated before they are shown. */
export function readCompareBench(catalog: CatalogDto): { rows: SolvedBuildDto[]; staleRows: SolvedBuildDto[]; notices: Notice[] } {
  const empty = { rows: [], staleRows: [], notices: [] };
  if (typeof localStorage === "undefined") return empty;
  try {
    const raw = localStorage.getItem(compareBenchKey(catalog.dataManifest.profile.id));
    if (!raw) return empty;
    const value: unknown = JSON.parse(raw);
    if (!isRecord(value) || value.version !== 1 || !Array.isArray(value.rows)) return empty;
    const rows = value.rows.flatMap((row) => {
      const checked = validateStoredBuild(row, catalog.dataManifest.rules);
      return "value" in checked ? [checked.value] : [];
    });
    const notices: Notice[] = rows.length === value.rows.length ? [] : [{
      scope: "global", tone: "warning", message: "Some saved comparison builds were discarded because their data is invalid.",
    }];
    const kept = rows.slice(0, MAX_COMPARE_PINS);
    const current = value.datasetVersion === catalog.dataManifest.datasetVersion
      && value.schemaVersion === catalog.dataManifest.schemaVersion && value.modelVersion === catalog.dataManifest.modelVersion;
    return current ? { rows: kept, staleRows: [], notices } : { rows: [], staleRows: kept, notices };
  } catch {
    return empty;
  }
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
