import packageInfo from "../../package.json";
import { buildOptimizeRequest, rowFingerprint, STAT_KEYS } from "./session";
import { knownRequestFields } from "./presets";
import type { DesktopState } from "./state";
import { aowModelSupported, hasAowDamage } from "./format";

// Reports use an allowlist, never a dump of storage, manifests, or application state.
export function reproductionReport(state: DesktopState) {
  const normalized = buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  const request = knownRequestFields(normalized);
  const manifest = state.catalog?.dataManifest;
  const row = !state.resultsStale ? state.selected : null;
  const aowAvailable = row && hasAowDamage(row, aowModelSupported(manifest));
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
        aowFirstHitDamage: aowAvailable ? row.aowFirstHitDamage : null,
        aowFullSequenceDamage: aowAvailable ? row.aowFullSequenceDamage : null,
      } : null },
    error: state.error ?? state.catalogError,
  };
  return JSON.stringify(report, (_key, value: unknown) => typeof value === "string" ? reportText(value) : value, 2);
}

// Free text (errors, imported names) can carry paths or secrets; the rest of the report is an
// allowlist. A match omits the whole string. Dotted numbers are not treated as addresses,
// because mod versions such as 3.0.0.1 look the same.
const PRIVATE_TEXT = new RegExp([
  String.raw`\b[a-z]:[\\/]`, // drive path
  String.raw`\\\\`, // network share
  String.raw`[\w.-]+\\[\w.$-]+`, // any backslash path, including one without a drive (Users\name)
  String.raw`file:\/\/`, String.raw`\/(?:home|Users|tmp|var|etc)\/`, String.raw`https?:\/\/`,
  String.raw`\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b`, // e-mail address
  String.raw`\b(?:token|password|secret|authorization|api[_ -]?key)\s*[:=]`, String.raw`\bbearer\s+\S`,
  String.raw`[A-Za-z0-9+/_-]{40,}`, // key- or token-like run
].join("|"), "i");

function reportText(value: string): string {
  if (PRIVATE_TEXT.test(value)) {
    return "[Omitted: possible personal path, address, or credential.]";
  }
  return value.length > 2000 ? `${value.slice(0, 2000)} [truncated]` : value;
}
