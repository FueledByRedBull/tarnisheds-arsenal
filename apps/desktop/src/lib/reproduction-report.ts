import packageInfo from "../../package.json";
import { buildOptimizeRequest, rowFingerprint, STAT_KEYS } from "./session";
import { defaultRequest, type DesktopState } from "./state";

// Reports use an allowlist, never a dump of storage, manifests, or application state.
export function reproductionReport(state: DesktopState) {
  const normalized = buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  const request = Object.fromEntries(Object.keys(defaultRequest)
    .filter(key => key in normalized)
    .filter(key => key !== "filters")
    .map(key => [key, normalized[key as keyof typeof normalized]]));
  request.filters = { version: normalized.filters.version,
    entries: normalized.filters.entries.map(({ dimension, id, mode }) => ({ dimension, id, mode })) };
  const manifest = state.catalog?.dataManifest;
  const row = !state.resultsStale ? state.selected : null;
  const report = {
    format: "tarnisheds-arsenal-reproduction", version: 1,
    appVersion: packageInfo.version,
    capturedAt: new Date().toISOString(),
    workspace: state.activeWorkspace,
    profileId: state.request.profileId,
    snapshot: manifest ? {
      id: manifest.id, schemaVersion: manifest.schemaVersion,
      datasetVersion: manifest.datasetVersion, modelVersion: manifest.modelVersion,
      gameVersion: manifest.profile.gameVersion, modVersion: manifest.profile.modVersion,
    } : null,
    context: "Current normalized inputs, not a history of previous or failed requests. The displayed selection may have been loaded from a saved build; capture does not recompute it.",
    request,
    results: { stale: state.resultsStale, searching: state.isSearching, exporting: state.isExporting,
      selected: row ? {
        fingerprint: rowFingerprint(row), weaponId: row.weaponId, weaponName: row.weaponName,
        affinity: row.affinity, upgrade: row.upgrade, isSomber: row.isSomber,
        aowId: row.aowId, aowName: row.aowName,
        stats: Object.fromEntries(STAT_KEYS.map(key => [key, row.stats[key]])),
        ar: Object.fromEntries(["physical", "magic", "fire", "lightning", "holy", "total"].map(key => [key, row.ar[key as keyof typeof row.ar]])),
        bleedBuildup: row.bleedBuildup,
        aowFirstHitDamage: manifest?.capabilities.aowDamage ? row.aowFirstHitDamage : null,
        aowFullSequenceDamage: manifest?.capabilities.aowDamage ? row.aowFullSequenceDamage : null,
      } : null },
    error: state.error ?? state.catalogError,
  };
  return JSON.stringify(report, (_key, value: unknown) => typeof value === "string" ? reportText(value) : value, 2);
}

function reportText(value: string): string {
  if (/(?:[a-z]:[\\/]|\\\\|file:\/\/|\/(?:home|Users|tmp|var|etc)\/|https?:\/\/|\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b|\b(?:token|password|secret|authorization|api[_ -]?key)\s*[:=])/i.test(value)) {
    return "[Omitted: possible personal path, address, or credential.]";
  }
  return value.length > 2000 ? `${value.slice(0, 2000)} [truncated]` : value;
}
