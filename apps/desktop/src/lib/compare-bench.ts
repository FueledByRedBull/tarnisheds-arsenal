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
    // Only the rows that can be kept are validated, so an oversized stored list costs no more to read.
    const candidates = value.rows.slice(0, MAX_COMPARE_PINS);
    const kept = candidates.flatMap((row) => {
      const checked = validateStoredBuild(row, catalog.dataManifest.rules);
      return "value" in checked ? [checked.value] : [];
    });
    const notices: Notice[] = [];
    if (kept.length !== candidates.length) {
      notices.push({ scope: "global", tone: "warning", message: "Some saved comparison builds were discarded because their data is invalid." });
    }
    const over = value.rows.length - MAX_COMPARE_PINS;
    if (over > 0) {
      notices.push({ scope: "global", tone: "warning",
        message: `Compare holds ${MAX_COMPARE_PINS} pinned builds; ${over} saved ${over === 1 ? "pin" : "pins"} over that limit ${over === 1 ? "was" : "were"} not restored.` });
    }
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
