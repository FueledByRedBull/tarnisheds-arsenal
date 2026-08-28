import { Clipboard, Download, GitCompareArrows, LockKeyhole, Pencil, Radar, Route, Save, Target, Trash2, Upload } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { compactNumber, fixed1, metricForObjective, objectiveLabel, statLine } from "../../lib/format";
import {
  deleteBuildPreset,
  downloadPresetJson,
  importBuildPreset,
  loadBuildPreset,
  parsePresetText,
  previewPresetImport,
  replaceImportedBuildPreset,
  renameBuildPreset,
  saveBuildPreset,
  savedBuildIndex,
  shareTextForPreset,
} from "../../lib/presets";
import { budgetSnapshot, buildOptimizeRequest } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { AowRouteDto, BuildPreset, CatalogDto, OptimizeRequestDto, SavedBuildIndexEntryV1, SolvedBuildDto, StatusBuildupDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";
import { ScalingTokens, StatusTokens } from "../shared/BuildMetricTokens";
import packageInfo from "../../../package.json";

export function Inspector() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const request = useDesktopStore((state) => state.request);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  const useRowAsLocks = useDesktopStore((state) => state.useRowAsLocks);
  const snapshot = budgetSnapshot(catalog, request);

  async function lockSelected() {
    if (!selected) return;
    useRowAsLocks(selected);
    await runSearchFromStore();
  }

  return (
    <aside className="inspector">
      <div className="inspector-title">
        <Target size={17} />
        <span>Build detail</span>
      </div>
      <div className="detail-block">
        <span>Stat Budget</span>
        <strong>Level {snapshot.level} / +{snapshot.levelUps} level ups</strong>
        <small>Redistrib {snapshot.redistributable} / Free {snapshot.freePoints} / Total points {snapshot.total}</small>
      </div>
      <div className="detail-block">
        <span>Lock State</span>
        <strong>{lockedStatMode && request.lockStr !== null ? "Exact upgrade and stat locks active" : "Open or partial locks"}</strong>
        <small>
          {request.lockStr === null
            ? "No captured combat stat locks."
            : `STR ${request.lockStr} DEX ${request.lockDex} INT ${request.lockInt} FAI ${request.lockFai} ARC ${request.lockArc}`}
        </small>
      </div>
      {selected ? (
        <>
          <div className="selected-build">
            <strong>{selected.weaponName}</strong>
            <span>{selected.affinity} / {selected.aowName ?? "Native"} / +{selected.upgrade}</span>
            {resultsStale ? <small className="stale-label">Previous query build</small> : null}
          </div>
          <div className="metric-grid">
            <Metric label={objectiveLabel(request.objective)} value={fixed1(metricForObjective(selected, request.objective))} />
            <Metric label="AR" value={compactNumber(selected.ar.total)} />
            <Metric
              label={catalog?.dataManifest.capabilities.aowRoutes ? "Raw AoW" : "AoW model"}
              value={catalog?.dataManifest.capabilities.aowRoutes
                ? compactNumber(selected.aowFullSequenceDamage)
                : "Not mapped"}
            />
          </div>
          <ModelCoverage />
          <div className="detail-block">
            <span>Combat Stats</span>
            <strong>{statLine(selected)}</strong>
          </div>
          <div className="detail-block build-token-detail">
            <span>Attribute Scaling</span>
            <ScalingTokens
              scaling={selected.effectiveScaling}
              extended={catalog?.dataManifest.rules.extendedScalingGrades ?? false}
            />
          </div>
          <div className="detail-block build-token-detail">
            <span>Status Buildup</span>
            <StatusTokens row={selected} />
          </div>
          <div className="detail-block split">
            <span>AR Split</span>
            <small>
              PHY {compactNumber(selected.ar.physical)} / MAG {compactNumber(selected.ar.magic)} /
              FIRE {compactNumber(selected.ar.fire)} / LIT {compactNumber(selected.ar.lightning)} /
              HOLY {compactNumber(selected.ar.holy)}
            </small>
          </div>
          <AowRouteDetails route={selected.aowRoute} />
          <div className="inspector-actions stacked">
            <button type="button" onClick={lockSelected}><LockKeyhole size={15} />Use as search locks</button>
            <button type="button" onClick={() => setWorkspace("compare")} disabled={resultsStale} title={resultsStale ? "Update rankings before comparing" : undefined}><GitCompareArrows size={15} />Compare</button>
            <button type="button" onClick={() => setWorkspace("paths")} disabled={resultsStale} title={resultsStale ? "Update rankings before tracing paths" : undefined}><Route size={15} />Paths</button>
            <button type="button" onClick={() => setWorkspace("affinity_watch")} disabled={resultsStale} title={resultsStale ? "Update rankings before watching affinities" : undefined}><Radar size={15} />Affinity Watch</button>
          </div>
        </>
      ) : (
        <div className="empty-state compact">
          <strong>No build selected</strong>
          <span>Select a ranked row.</span>
          <button type="button" onClick={() => setWorkspace("rankings")}>Go to Rankings</button>
        </div>
      )}
      <SavedBuildPanel />
    </aside>
  );
}

function ModelCoverage() {
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const aowModelUnavailable = catalog && (
    !catalog.dataManifest.capabilities.aowDamage ||
    !catalog.dataManifest.capabilities.aowRoutes
  );
  const profileRules = catalog?.dataManifest.rules;
  const objectiveWarning = aowModelUnavailable
    ? "Weapon AR, status, passives, affinities, and AoW compatibility are modeled. This profile's AoW hit and route damage is not mapped, so those objectives are unavailable."
    : request.objective === "max_ar_plus_bleed"
      ? "Buildup is modeled, but enemy resistance growth and proc explosion damage are not."
      : request.objective === "aow_first_hit" || request.objective === "aow_full_sequence"
        ? "Legal route damage, status, buff timing, physical attribute, and stamina are reported. Stamina is not optimized and unsupported effects remain warnings."
        : "Attack rating is calculated before enemy defense and negation.";
  return (
    <details className="model-coverage">
      <summary>Model coverage and assumptions</summary>
      <p>{objectiveWarning}</p>
      {profileRules?.zeroAttackElementUsesWeaponScaling ? (
        <p>Convergence weapons with correction row 0 apply each declared nonzero attribute scaling to each nonzero damage component.</p>
      ) : null}
      <p>Temporary buff stacking is not a universal layer. Values are raw model outputs, not expected damage against a specific enemy.</p>
      <small>
        App {packageInfo.version} · {catalog?.dataManifest.profile.displayName ?? "unknown profile"} · dataset {catalog?.dataManifest.datasetVersion ?? "unknown"} · schema {catalog?.dataManifest.schemaVersion ?? "unknown"} · model {catalog?.dataManifest.modelVersion ?? "unknown"}
      </small>
      <small>
        {request.twoHanding ? "Two-handed" : "One-handed"} · {profileRules?.scadutreeScaling
          ? request.dlcScaling ? `Scadutree ${request.scadutreeLevel}` : "DLC scaling off"
          : "Scadutree unavailable"} · upgrade {request.exactUpgrade ? "exact" : "open range"}
      </small>
    </details>
  );
}

function AowRouteDetails({ route }: { route: AowRouteDto | null }) {
  if (!route) return null;
  return (
    <details className="aow-route-details">
      <summary>
        <span>{route.routeLabel}</span>
        <strong>{compactNumber(route.totalDamage.total)} dmg / {fixed1(route.totalStaminaCost)} stamina</strong>
      </summary>
      {route.buffActivationActionId ? (
        <small>Weapon buff activates at: {route.buffActivationActionId}</small>
      ) : null}
      <small>Total status: {formatStatus(route.totalStatusBuildup)}</small>
      <div className="aow-actions">
        {route.actions.map((action) => (
          <div className="aow-action" key={`${action.actionOrder}-${action.actionId}`}>
            <b>{action.actionOrder}. {action.actionId}</b>
            <small>{fixed1(action.staminaCost)} stamina</small>
            {action.hits.map((hit) => (
              <div className="aow-hit" key={`${hit.sheetRow}-${hit.hitOrder}`}>
                <span>{hit.rawName}</span>
                <strong>{compactNumber(hit.damage.total)} / {hit.physicalAttackAttribute.replaceAll("_", " ")}</strong>
                <small>{formatStatus(hit.statusBuildup)}{hit.buffActive ? " / buff active" : ""}</small>
                {hit.effects
                  .filter((effect) => effect.role === "per_hit_status" || !effect.isSupported)
                  .map((effect) => (
                    <small key={`${hit.sheetRow}-${effect.effectId}`} className={effect.isSupported ? "" : "warning-text"}>
                      {effect.effectName || `Effect ${effect.effectId}`}: {effect.reason}
                    </small>
                  ))}
                {hit.warnings.map((warning) => <small className="warning-text" key={warning}>{warning}</small>)}
              </div>
            ))}
          </div>
        ))}
      </div>
    </details>
  );
}

function formatStatus(status: StatusBuildupDto): string {
  const values = [
    ["bleed", status.bleed],
    ["frost", status.frost],
    ["poison", status.poison],
    ["rot", status.scarletRot],
    ["sleep", status.sleep],
    ["madness", status.madness],
    ["death", status.death],
  ].filter((entry) => Number(entry[1]) > 0);
  return values.length ? values.map(([label, value]) => `${label} ${compactNumber(Number(value))}`).join(" / ") : "none";
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-tile">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function SavedBuildPanel() {
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const selected = useDesktopStore((state) => state.selected);
  const compareTarget = useDesktopStore((state) => state.compareTarget);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const hydrate = useDesktopStore((state) => state.loadBuildPreset);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const [entries, setEntries] = useState<SavedBuildIndexEntryV1[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("Build Preset");
  const [importText, setImportText] = useState("");
  const [deleteArmedId, setDeleteArmedId] = useState<string | null>(null);
  const [replaceImport, setReplaceImport] = useState(false);
  const [importDataMode, setImportDataMode] = useState<"stale" | "migrate">("stale");
  const [isMigrating, setMigrating] = useState(false);
  const dataVersion = catalog
    ? `${catalog.dataManifest.profile.id}:${catalog.dataManifest.schemaVersion}:${catalog.dataManifest.datasetVersion}:${catalog.dataManifest.modelVersion}`
    : "unknown";

  function refresh() {
    const next = savedBuildIndex().builds;
    setEntries(next);
    if (!selectedId && next[0]) setSelectedId(next[0].id);
  }

  useEffect(refresh, []);

  function currentPreset() {
    return selectedId ? loadBuildPreset(selectedId) : null;
  }

  function saveNew() {
    try {
      const preset = saveBuildPreset({ name, request, selectedBuild: selected, compareTarget, compareBench, dataVersion });
      setSelectedId(preset.id);
      refresh();
      pushNotice({ scope: "global", tone: "success", message: `Saved ${preset.name}.` });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  function updateCurrent() {
    if (!selectedId) return;
    try {
      const preset = saveBuildPreset({ id: selectedId, name, request, selectedBuild: selected, compareTarget, compareBench, dataVersion });
      refresh();
      pushNotice({ scope: "global", tone: "success", message: `Updated ${preset.name}.` });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  function loadCurrent() {
    const preset = currentPreset();
    if (!preset) return;
    if (preset.profileId !== request.profileId) {
      pushNotice({
        scope: "global",
        tone: "warning",
        message: `${preset.name} belongs to ${preset.profileId}. Switch the game profile first; cross-profile builds are never loaded silently.`,
      });
      return;
    }
    if (dataVersion !== "unknown" && preset.dataVersion !== dataVersion) {
      hydrate({ ...preset, selectedBuild: null, compareTarget: null, compareBench: [] });
      pushNotice({
        scope: "global",
        tone: "warning",
        message: `Loaded ${preset.name} inputs only: saved data ${preset.dataVersion} differs from ${dataVersion}. Rerun the search.`,
      });
      return;
    }
    hydrate(preset);
  }

  async function migratePreset(preset: BuildPreset): Promise<BuildPreset | null> {
    if (!catalog) {
      setError("Current catalog metadata is unavailable; retry after game data finishes loading.");
      return null;
    }
    if (preset.profileId !== request.profileId) {
      setError(`This preset belongs to ${preset.profileId}. Switch profiles before migrating it.`);
      return null;
    }
    setMigrating(true);
    setError(null);
    const migration = migratePresetRequest(preset.request, catalog);
    try {
      hydrate({ ...preset, request: migration.request, selectedBuild: null, compareTarget: null, compareBench: [] });
      const state = useDesktopStore.getState();
      const base = buildOptimizeRequest(catalog, state.request, state.lockedStatMode);
      const issues = [...migration.issues];
      const recompute = async (label: string, row: SolvedBuildDto | null) => {
        if (!row) return null;
        if (!catalog.weaponNames.includes(row.weaponName)) {
          issues.push(`${label} weapon '${row.weaponName}' no longer exists`);
          return null;
        }
        if (!catalog.affinityNames.includes(row.affinity)) {
          issues.push(`${label} affinity '${row.affinity}' no longer exists`);
          return null;
        }
        if (row.aowName && !catalog.aowNames.includes(row.aowName)) {
          issues.push(`${label} skill '${row.aowName}' no longer exists`);
          return null;
        }
        const solved = await api.solveBuild(base, row.weaponName, row.affinity, row.aowName);
        if (!solved) issues.push(`${label} configuration is no longer legal for the migrated request`);
        return solved;
      };
      const migratedSelected = await recompute("Selected build", preset.selectedBuild);
      const migratedCompare = await recompute("Compare build", preset.compareTarget);
      const migratedBench = (await Promise.all(preset.compareBench.map((row, index) => recompute(`Compare bench ${index + 1}`, row))))
        .filter((row): row is SolvedBuildDto => row !== null);
      const migrated = saveBuildPreset({
        id: preset.id,
        name: preset.name,
        request: useDesktopStore.getState().request,
        selectedBuild: migratedSelected,
        compareTarget: migratedCompare,
        compareBench: migratedBench,
        dataVersion,
      });
      hydrate(migrated);
      setSelectedId(migrated.id);
      setName(migrated.name);
      refresh();
      pushNotice({
        scope: "global",
        tone: issues.length ? "warning" : "success",
        message: issues.length
          ? `Migrated ${migrated.name}; cleared or unresolved: ${issues.join("; ")}.`
          : `Migrated ${migrated.name} and recomputed its selected and compare builds on current data.`,
      });
      return migrated;
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      return null;
    } finally {
      setMigrating(false);
    }
  }

  function renameCurrent() {
    if (!selectedId) return;
    try {
      renameBuildPreset(selectedId, name);
      refresh();
      pushNotice({ scope: "global", tone: "success", message: "Saved build renamed." });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  function deleteCurrent() {
    if (!selectedId) return;
    if (deleteArmedId !== selectedId) {
      setDeleteArmedId(selectedId);
      return;
    }
    const deletedName = currentPreset()?.name ?? "saved build";
    deleteBuildPreset(selectedId);
    setSelectedId("");
    setDeleteArmedId(null);
    refresh();
    pushNotice({ scope: "global", tone: "success", message: `Deleted ${deletedName}.` });
  }

  function selectPreset(id: string) {
    setSelectedId(id);
    setDeleteArmedId(null);
    const preset = id ? loadBuildPreset(id) : null;
    setName(preset?.name ?? "Build Preset");
  }

  async function copyCurrent() {
    const preset = currentPreset();
    if (!preset) return;
    try {
      await navigator.clipboard.writeText(shareTextForPreset(preset));
      pushNotice({ scope: "global", tone: "success", message: "Copied share text." });
    } catch {
      setError("Could not copy to the clipboard. Check clipboard permission, then retry or use Export instead.");
    }
  }

  function exportCurrent() {
    const preset = currentPreset();
    if (preset) downloadPresetJson(preset);
  }

  async function importCurrent() {
    try {
      const parsed = parsePresetText(importText);
      const preset = replaceImport ? replaceImportedBuildPreset(parsed) : importBuildPreset(parsed);
      setSelectedId(preset.id);
      setName(preset.name);
      setImportText("");
      setReplaceImport(false);
      setImportDataMode("stale");
      refresh();
      if (preset.dataVersion !== dataVersion && importDataMode === "migrate") {
        await migratePreset(preset);
      } else {
        pushNotice({
          scope: "global",
          tone: preset.dataVersion === dataVersion ? "success" : "warning",
          message: preset.dataVersion === dataVersion
            ? `Imported ${preset.name}.`
            : `Imported ${preset.name} as an explicitly stale snapshot; solved rows will not be trusted when loaded.`,
        });
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  const importPreview = useMemo(() => {
    if (!importText.trim()) return null;
    try {
      return { value: previewPresetImport(importText), error: null };
    } catch (error) {
      return { value: null, error: error instanceof Error ? error.message : String(error) };
    }
  }, [importText]);
  const selectedPreset = currentPreset();
  const selectedPresetStale = Boolean(selectedPreset && dataVersion !== "unknown" && selectedPreset.dataVersion !== dataVersion);

  return (
    <div className="saved-builds">
      <div className="inspector-title">
        <Save size={17} />
        <span>Saved Builds</span>
      </div>
      <label>
        Name
        <input value={name} onChange={(event) => setName(event.target.value)} />
      </label>
      <label>
        Saved
        <select value={selectedId} onChange={(event) => selectPreset(event.target.value)}>
          <option value="">None</option>
          {entries.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name} — {entry.profileId} · {entry.dataVersion === dataVersion ? "current data" : "different data"}
            </option>
          ))}
        </select>
      </label>
      {selectedId ? <small className="saved-build-status">{presetVersionLabel(currentPreset()?.dataVersion, dataVersion)}</small> : null}
      <div className="inspector-actions stacked">
        <button type="button" onClick={saveNew}><Save size={15} />Save new</button>
        <button type="button" onClick={updateCurrent} disabled={!selectedId}><Save size={15} />Update selected</button>
        <button type="button" onClick={loadCurrent} disabled={!selectedId || isMigrating}><Upload size={15} />{selectedPresetStale ? "Load inputs only" : "Load"}</button>
        {selectedPresetStale ? (
          <button type="button" onClick={() => selectedPreset && void migratePreset(selectedPreset)} disabled={isMigrating}>
            <Upload size={15} />{isMigrating ? "Migrating..." : "Migrate data"}
          </button>
        ) : null}
        <button type="button" onClick={renameCurrent} disabled={!selectedId}><Pencil size={15} />Rename</button>
        <button type="button" onClick={deleteCurrent} disabled={!selectedId}>
          <Trash2 size={15} />{deleteArmedId === selectedId ? "Confirm Delete" : "Delete"}
        </button>
        <button type="button" onClick={exportCurrent} disabled={!selectedId}><Download size={15} />Export</button>
        <button type="button" onClick={copyCurrent} disabled={!selectedId}><Clipboard size={15} />Copy Share</button>
      </div>
      <label>
        Import JSON or Share Text
        <textarea value={importText} onChange={(event) => setImportText(event.target.value)} />
      </label>
      {importPreview?.value ? (
        <div className="import-preview">
          <strong>{importPreview.value.preset.name}</strong>
          <span>{presetVersionLabel(importPreview.value.preset.dataVersion, dataVersion)}</span>
          <small>{importPreview.value.bytes.toLocaleString()} bytes · Level {importPreview.value.preset.request.characterLevel}</small>
          {importPreview.value.preset.dataVersion !== dataVersion ? (
            <label>
              Data handling
              <select value={importDataMode} onChange={(event) => setImportDataMode(event.target.value as "stale" | "migrate")}>
                <option value="stale">Keep stale snapshot (safe)</option>
                <option value="migrate">Migrate and recompute now</option>
              </select>
            </label>
          ) : null}
          {importPreview.value.idConflict || importPreview.value.nameConflict ? (
            <label>
              Conflict handling
              <select value={replaceImport ? "replace" : "copy"} onChange={(event) => setReplaceImport(event.target.value === "replace")}>
                <option value="copy">Keep both (safe copy)</option>
                {importPreview.value.idConflict ? <option value="replace">Replace matching ID</option> : null}
              </select>
            </label>
          ) : null}
        </div>
      ) : importPreview?.error ? <small className="warning-text">{importPreview.error}</small> : null}
      <button className="clear-locks" type="button" onClick={() => void importCurrent()} disabled={!importPreview?.value || isMigrating}>
        <Upload size={15} />{isMigrating ? "Migrating..." : "Import"}
      </button>
    </div>
  );
}

function migratePresetRequest(request: OptimizeRequestDto, catalog: CatalogDto): { request: OptimizeRequestDto; issues: string[] } {
  const migrated = { ...request };
  const issues: string[] = [];
  if (!catalog.classes.some((entry) => entry.name === migrated.className)) {
    issues.push(`class '${migrated.className}' no longer exists`);
    const replacement = catalog.classes[0];
    migrated.className = replacement?.name ?? "Samurai";
    if (replacement) {
      migrated.characterLevel = replacement.baseLevel;
      migrated.vig = replacement.baseStats.vig;
      migrated.mnd = replacement.baseStats.mnd;
      migrated.end = replacement.baseStats.end;
      migrated.strStat = replacement.baseStats.strStat;
      migrated.dex = replacement.baseStats.dex;
      migrated.intStat = replacement.baseStats.intStat;
      migrated.fai = replacement.baseStats.fai;
      migrated.arc = replacement.baseStats.arc;
      issues.push("character stats reset to the replacement class baseline");
    }
  }
  if (migrated.weaponTypeKey && !catalog.weaponTypeKeys.includes(migrated.weaponTypeKey)) {
    issues.push(`weapon type '${migrated.weaponTypeKey}' no longer exists`);
    migrated.weaponTypeKey = null;
  }
  if (migrated.weaponName && !catalog.weaponNames.includes(migrated.weaponName)) {
    issues.push(`weapon '${migrated.weaponName}' no longer exists`);
    migrated.weaponName = null;
    migrated.affinity = null;
    migrated.aowName = null;
  }
  if (migrated.affinity && !catalog.affinityNames.includes(migrated.affinity)) {
    issues.push(`affinity '${migrated.affinity}' no longer exists`);
    migrated.affinity = null;
    migrated.aowName = null;
  }
  if (migrated.aowName && !catalog.aowNames.includes(migrated.aowName)) {
    issues.push(`skill '${migrated.aowName}' no longer exists`);
    migrated.aowName = null;
  }
  return { request: migrated, issues };
}

function presetVersionLabel(savedVersion: string | undefined, currentVersion: string): string {
  if (!savedVersion) return "Saved metadata unavailable";
  const [schema, dataset, model] = savedVersion.split(":");
  const status = savedVersion === currentVersion ? "Current" : "Stale — inputs load, solved rows are discarded";
  return `${status} · dataset ${dataset ?? "unknown"} · schema ${schema ?? "unknown"} · model ${model ?? "unknown"}`;
}
