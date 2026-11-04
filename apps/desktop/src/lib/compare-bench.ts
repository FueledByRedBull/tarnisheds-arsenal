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
    // Rows are validated in order only until the bench is full, so an oversized stored list costs
    // no more to read; an invalid row does not take a valid one's slot.
    const kept: SolvedBuildDto[] = [];
    let read = 0;
    while (read < value.rows.length && kept.length < MAX_COMPARE_PINS) {
      const checked = validateStoredBuild(value.rows[read], catalog.dataManifest.rules);
      read += 1;
      if ("value" in checked) kept.push(checked.value);
    }
    const notices: Notice[] = [];
    if (kept.length !== read) {
      notices.push({ scope: "global", tone: "warning", message: "Some saved comparison builds were discarded because their data is invalid." });
    }
    const over = value.rows.length - read;
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
