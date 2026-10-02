import { Check, ChevronDown, Clipboard, Download, LockKeyhole, Pencil, Pin, Save, Target, Trash2, Upload } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { activationRequest } from "../../lib/preset-activation";
import { useWeaponProfileResource } from "../../lib/hooks";
import { compactNumber, fixed1, hasAowDamage, metricForObjective, objectiveLabel, statLine, statLockLine } from "../../lib/format";
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
import { budgetSnapshot, buildOptimizeRequest, hasCombatStatLocks, rowFingerprint } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { AowRouteDto, BuildPreset, CatalogDto, OptimizeRequestDto, SavedBuildIndexEntryV1, SolvedBuildDto, StatusBuildupDto, WeaponProfileDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";
import { ScalingTokens, StatusTokens } from "../shared/BuildMetricTokens";
import packageInfo from "../../../package.json";
import { explainBuild, leadOver } from "../../lib/build-explanation";
import { ReproductionReport } from "../shared/ReproductionReport";
import { SavedBuildRecovery } from "../shared/SavedBuildRecovery";

export function Inspector() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const rows = useDesktopStore((state) => state.rows);
  const rowsObjective = useDesktopStore((state) => state.rowsObjective);
  const request = useDesktopStore((state) => state.request);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  const applyRowLocks = useDesktopStore((state) => state.useRowAsLocks);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const toggleCompareBench = useDesktopStore((state) => state.toggleCompareBench);
  const snapshot = budgetSnapshot(catalog, request);
  const fixedStats = catalog?.dataManifest.capabilities.classBudget === false;
  const aowModelSupported = Boolean(catalog?.dataManifest.capabilities.aowDamage && catalog.dataManifest.capabilities.aowRoutes);
  const aowAvailable = selected && hasAowDamage(selected, aowModelSupported);
  const selectedMetric = selected ? metricForObjective(selected, request.objective, aowModelSupported) : null;
  const pinned = Boolean(selected && compareBench.some((entry) => rowFingerprint(entry) === rowFingerprint(selected)));
  const rank = selected ? rows.findIndex((row) => rowFingerprint(row) === rowFingerprint(selected)) : -1;
  const leaderMetric = rows[0] ? metricForObjective(rows[0], request.objective, aowModelSupported) : null;
  const behindLeader = rank > 0 && leaderMetric !== null && selectedMetric !== null ? leaderMetric - selectedMetric : null;
  // The trust line: how far #1 leads #2, and the stat changes it asks of the entered build.
  const runnerUp = rank === 0 ? rows[1] ?? null : null;
  const lead = selected && runnerUp ? leadOver(selected, runnerUp, request.objective, aowModelSupported) : null;
  const statChanges = selected && !fixedStats && !resultsStale ? respecChanges(selected, request) : null;
  const modelWarnings = [...new Set(selected?.aowRoute?.actions.flatMap(
    (action) => action.hits.flatMap((hit) => hit.warnings),
  ) ?? [])];
  // A new selection updates the panel in place and fades it in, instead of rebuilding it.
  const detail = useRef<HTMLDivElement>(null);
  const selectedKey = rowFingerprint(selected);
  useEffect(() => {
    if (!selectedKey || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    detail.current?.animate(
      [{ opacity: 0.35, transform: "translateY(4px)" }, { opacity: 1, transform: "none" }],
      { duration: 200, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },
    );
  }, [selectedKey]);
  const weaponResource = useWeaponProfileResource(request.profileId, selected?.weaponName ?? null, selected?.affinity ?? null);
  const weaponProfile = weaponResource.profile;

  async function lockSelected() {
    if (!selected) return;
    applyRowLocks(selected);
    await runSearchFromStore();
  }

  return (
    <aside className="inspector">
      <div className="inspector-title">
        <Target size={17} />
        <span>Build detail</span>
      </div>
      {selected ? (
        <div className="selection-detail" ref={detail}>
          <div className="selected-build">
            <div className="selected-build-head">
              <div className="selected-build-title">
                <strong>{selected.weaponName}</strong>
                {/* Shown only while the header is stuck (styles.css), so the answer stays in view. */}
                <span className="selected-build-metric" aria-hidden="true">{selectedMetric === null ? "" : fixed1(selectedMetric)}</span>
              </div>
              <span>{selected.affinity} / {selected.aowName ?? "Unspecified skill"} / +{selected.upgrade}</span>
              {resultsStale ? <small className="stale-label">Previous query build</small> : null}
            </div>
          </div>
          {!aowAvailable || modelWarnings.length ? (
            <div className="selection-notes">
              {!aowAvailable ? <small className="tone-info">Skill damage isn't modeled for this loadout.</small> : null}
              {modelWarnings.map((warning) => <small className="tone-warning" key={warning}>{warning}</small>)}
            </div>
          ) : null}
          {weaponResource.status === "loading" ? <WeaponPoiseSkeleton /> : null}
          {weaponResource.status === "error" ? <div role="alert">
            <small>{weaponResource.error}</small>
            <button type="button" onClick={weaponResource.retry}>Retry weapon profile</button>
          </div> : null}
          <div className="metric-grid">
            <Metric label={objectiveLabel(request.objective)} value={selectedMetric === null ? null : fixed1(selectedMetric)} />
            {request.objective !== "max_ar" ? <Metric label="AR" value={fixed1(selected.ar.total)} /> : null}
            <Metric
              label={catalog?.dataManifest.capabilities.aowRoutes ? "Raw AoW" : "AoW model"}
              value={aowAvailable ? compactNumber(selected.aowFullSequenceDamage) : null}
            />
          </div>
          {rank >= 0 && !resultsStale ? (
            <p className="rank-context">
              Rank <strong>{rank + 1}</strong> of {rows.length}
              {rank === 0
                ? lead === null ? " · best for this query"
                  : lead < 0.05 ? " · less than 0.05 ahead of #2" : ` · ${fixed1(lead)} ahead of #2`
                : behindLeader === null ? ""
                  : behindLeader < 0.05 ? " · less than 0.05 behind #1" : ` · ${fixed1(behindLeader)} behind #1`}
            </p>
          ) : null}
          {rank >= 0 && !resultsStale && rowsObjective !== null ? (
            <p className="rank-proof">Exact ranking of every legal setup this query allows.</p>
          ) : null}
          {statChanges ? (
            <div className="detail-block stat-changes">
              <span>Stat changes</span>
              <strong>{statChanges.length ? statChanges.join(", ") : "None: uses your stats as entered"}</strong>
            </div>
          ) : null}
          <div className="inspector-actions stacked">
            <button type="button" onClick={lockSelected}><LockKeyhole size={15} />Lock stats to this build</button>
            <button
              type="button"
              onClick={() => {
                const panel = document.getElementById("saved-builds-panel") as HTMLDetailsElement | null;
                if (panel) panel.open = true;
                panel?.scrollIntoView({ block: "start" });
                panel?.querySelector("input")?.focus({ preventScroll: true });
              }}
            >
              <Save size={15} />Save build
            </button>
            <button
              type="button"
              aria-pressed={pinned}
              aria-label={`${pinned ? "Unpin" : "Pin"} ${selected.weaponName} for comparison`}
              onClick={() => toggleCompareBench(selected)}
            >
              <Pin size={15} />{pinned ? "Unpin compare" : "Pin for compare"}
            </button>
          </div>
          <div className="detail-block">
            <span>Combat Stats</span>
            <strong>{statLine(selected)}</strong>
          </div>
          <WeaponPoiseDetails profile={weaponProfile} route={aowAvailable ? selected.aowRoute : null} twoHanding={request.twoHanding} />
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
          <AowRouteDetails route={aowAvailable ? selected.aowRoute : null} />
          {!resultsStale ? <details className="model-coverage build-explanation">
            <summary>Why this build?</summary>
            {explainBuild(selected, buildOptimizeRequest(catalog, request, lockedStatMode), aowModelSupported, runnerUp).map(line => <p key={line}>{line}</p>)}
          </details> : null}
          <ModelCoverage />
        </div>
      ) : (
        <div className="empty-state compact">
          <strong>No build selected</strong>
          <span>Select a ranked row.</span>
          <button type="button" onClick={() => setWorkspace("rankings")}>Go to Rankings</button>
        </div>
      )}
      <div className="detail-block">
        <span>Stat Budget</span>
        <strong>{fixedStats
          ? `Fixed stat total ${snapshot.level}` : `Level ${snapshot.level} / +${snapshot.levelUps} level ups`}</strong>
        <small>{fixedStats
          ? "Entered combat stats are evaluated as-is; class budgets and redistribution are unavailable."
          : `${snapshot.redistributable} respec points · ${snapshot.freePoints} unspent · ${snapshot.total} total points`}</small>
      </div>
      <div className="detail-block">
        <span>Stat locks</span>
        <strong>{fixedStats
          ? "Fixed stats evaluated as entered"
          : lockedStatMode && hasCombatStatLocks(request) ? "Stat locks active" : "No stat locks"}</strong>
        <small>
          {fixedStats
            ? "This profile does not derive a class budget or redistribute combat stats."
            : !lockedStatMode || !hasCombatStatLocks(request)
              ? "Every combat stat is free to move."
              : statLockLine(request)}
        </small>
      </div>
      <SavedBuildPanel />
      <details className="model-coverage report-problem">
        <summary>Report a problem</summary>
        <ReproductionReport />
      </details>
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

function WeaponPoiseSkeleton() {
  return (
    <div className="detail-block weapon-poise-detail" role="status" aria-label="Loading weapon data">
      <span className="skeleton" style={{ width: "38%" }} />
      <span className="skeleton" style={{ width: "72%", height: 14 }} />
      <div>
        {Array.from({ length: 6 }, (_, index) => <span className="skeleton" key={index} style={{ height: 34 }} />)}
      </div>
    </div>
  );
}

function WeaponPoiseDetails({ profile, route, twoHanding }: { profile: WeaponProfileDto | null; route: AowRouteDto | null; twoHanding: boolean }) {
  if (!profile || (!profile.moveCount && !profile.weight)) return null;
  const isTwoHanded = twoHanding || profile.forcesTwoHanding;
  const poise = isTwoHanded ? profile.twoHandedPoise : profile.oneHandedPoise;
  const aowPoise = route && route.totalPoiseDamage > 0
    ? route.actions.flatMap((action) => action.hits.map((hit) => hit.poiseDamage))
    : [];
  const poiseEntries: [string, string | number[]][] = [
    ["R1", poise.light],
    ["R2", poise.heavy],
    ["Charged R2", poise.chargedHeavy],
    ["Jumping R1", poise.jumpingLight],
    ["Jumping R2", poise.jumpingHeavy],
  ];
  if (aowPoise.length) poiseEntries.push(["AoW", aowPoise]);
  return (
    <div className="detail-block weapon-poise-detail">
      <span>Weapon data</span>
      <strong>Weight {fixed1(profile.weight)} · {profile.moveCount} mapped poise moves · {isTwoHanded ? "2H" : "1H"}</strong>
      <small className="poise-damage-note">PvE stance / poise damage</small>
      <div>
        {poiseEntries
          .map(([label, value]) => <small key={label}><b>{label}</b>{formatPoiseDamage(value)}</small>)}
      </div>
    </div>
  );
}

function AowRouteDetails({ route }: { route: AowRouteDto | null }) {
  if (!route) return null;
  const hasPoise = route.totalPoiseDamage > 0;
  return (
    <details className="aow-route-details">
      <summary>
        <span>{route.routeLabel}</span>
        <strong>{compactNumber(route.totalDamage.total)} dmg{hasPoise ? ` / ${formatPoiseNumber(route.totalPoiseDamage)} poise` : ""} / {fixed1(route.totalStaminaCost)} stamina</strong>
      </summary>
      {route.buffActivationActionId ? (
        <small>Weapon buff activates at: {route.buffActivationActionId}</small>
      ) : null}
      <small>Total status: {formatStatus(route.totalStatusBuildup)}</small>
      <div className="aow-actions">
        {route.actions.map((action) => (
          <div className="aow-action" key={`${action.actionOrder}-${action.actionId}`}>
            <b>{action.actionOrder}. {formatActionLabel(action.actionId)}</b>
            <small>{fixed1(action.staminaCost)} stamina</small>
            {action.hits.map((hit) => (
              <div className="aow-hit" key={`${hit.sheetRow}-${hit.hitOrder}`}>
                <span>{hit.rawName}</span>
                <strong>{compactNumber(hit.damage.total)} raw{hasPoise ? ` / ${formatPoiseNumber(hit.poiseDamage)} poise` : ""} / {hit.physicalAttackAttribute.replaceAll("_", " ")}</strong>
                <small>{formatStatus(hit.statusBuildup)}{hit.buffActive ? " / buff active" : ""}</small>
                {hit.effects
                  .filter((effect) => effect.role === "per_hit_status" || !effect.isSupported)
                  .map((effect) => (
                    <small key={`${hit.sheetRow}-${effect.effectId}`} className={effect.isSupported ? "" : "tone-warning"}>
                      {effect.effectName || `Effect ${effect.effectId}`}: {effect.reason}
                    </small>
                  ))}
                {hit.warnings.map((warning) => <small className="tone-warning" key={warning}>{warning}</small>)}
              </div>
            ))}
          </div>
        ))}
      </div>
    </details>
  );
}

function formatActionLabel(actionId: string): string {
  if (actionId === "activation") return "Skill";
  if (actionId === "r1") return "R1";
  if (actionId === "r2") return "R2";
  const stage = /^stage_(\d+)$/.exec(actionId);
  return stage ? `Stage ${stage[1]}` : actionId.replaceAll("_", " ");
}

function formatPoiseDamage(value: string | number[]): string {
  const hits = Array.isArray(value) ? value : value.match(/\d+(?:\.\d+)?/g)?.map(Number) ?? [];
  if (!hits.length) return "None";
  const breakdown = hits.map(formatPoiseNumber).join(" + ");
  return hits.length > 1
    ? `${breakdown} (${formatPoiseNumber(hits.reduce((total, hit) => total + hit, 0))} total)`
    : breakdown;
}

const formatPoiseNumber = (value: number) => Number(value.toFixed(2)).toString();

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

// A null value is unmodeled: it reads as "Unavailable", in a quieter style than a figure.
function Metric({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="metric-tile">
      <span>{label}</span>
      <strong className={value === null ? "metric-unavailable" : undefined}>{value ?? "Unavailable"}</strong>
    </div>
  );
}

const COMBAT_STATS = [["STR", "strStat"], ["DEX", "dex"], ["INT", "intStat"], ["FAI", "fai"], ["ARC", "arc"]] as const;

/** Each combat stat the build sets differently from the entered stats, as "DEX 15 → 22". */
function respecChanges(row: SolvedBuildDto, request: OptimizeRequestDto): string[] {
  return COMBAT_STATS.flatMap(([label, key]) => row.stats[key] === request[key] ? [] : [`${label} ${request[key]} → ${row.stats[key]}`]);
}

function SavedBuildPanel() {
  const catalog = useDesktopStore((state) => state.catalog);
  const catalogStatus = useDesktopStore((state) => state.catalogStatus);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const selected = useDesktopStore((state) => state.selected);
  const compareTarget = useDesktopStore((state) => state.compareTarget);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const hydrate = useDesktopStore((state) => state.loadBuildPreset);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const [entries, setEntries] = useState<SavedBuildIndexEntryV1[]>([]);
  const [libraryError, setLibraryError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState("");
  // Until the player types a name, a save is named after the selected build, as the game
  // names weapons ("Blood Uchigatana +25").
  const [typedName, setName] = useState<string | null>(null);
  const name = typedName ?? (selected ? `${selected.affinity === "Standard" ? "" : `${selected.affinity} `}${selected.weaponName} +${selected.upgrade}` : "Build Preset");
  const [copied, setCopied] = useState(false);
  const panel = useRef<HTMLDetailsElement>(null);
  const [importText, setImportText] = useState("");
  const [deleteArmedId, setDeleteArmedId] = useState<string | null>(null);
  const [replaceImport, setReplaceImport] = useState(false);
  const [importDataMode, setImportDataMode] = useState<"stale" | "migrate">("stale");
  const [isMigrating, setMigrating] = useState(false);
  const migrationController = useRef<AbortController | null>(null);
  const dataVersion = catalog
    ? `${catalog.dataManifest.profile.id}:${catalog.dataManifest.schemaVersion}:${catalog.dataManifest.datasetVersion}:${catalog.dataManifest.modelVersion}`
    : "unknown";
  const canSave = catalogStatus === "ready" && catalog?.dataManifest.profile.id === request.profileId;
  // Loading needs the verified catalog; startup and profile switches must finish first.
  const canLoad = catalogStatus === "ready";

  function refresh() {
    try {
      const next = savedBuildIndex().builds;
      setEntries(next);
      setLibraryError(null);
      setSelectedId(current => next.some(entry => entry.id === current) ? current : next[0]?.id ?? "");
    } catch (error) {
      setEntries([]);
      setSelectedId("");
      setLibraryError(error instanceof Error ? error.message : String(error));
    }
  }

  useEffect(refresh, []);
  useEffect(() => () => migrationController.current?.abort(), []);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  function cancelMigration() {
    migrationController.current?.abort();
    setMigrating(false);
  }

  function saveCurrent(id?: string) {
    if (!canSave) return;
    try {
      const { characterLevel, lockStr, lockDex, lockInt, lockFai, lockArc } = buildOptimizeRequest(catalog, request, lockedStatMode);
      const preset = saveBuildPreset({
        id, name, request: { ...request, characterLevel, lockStr, lockDex, lockInt, lockFai, lockArc },
        selectedBuild: resultsStale ? null : selected,
        compareTarget: resultsStale ? null : compareTarget,
        compareBench: resultsStale ? [] : compareBench,
        dataVersion,
      });
      cancelMigration();
      setSelectedId(preset.id);
      refresh();
      pushNotice({ scope: "global", tone: "success", message: `${id ? "Updated" : "Saved"} ${preset.name}.${resultsStale ? " Inputs only; rerun search for current results." : ""}` });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  async function loadCurrent() {
    if (!selectedPreset) return;
    migrationController.current?.abort();
    const controller = new AbortController();
    migrationController.current = controller;
    setMigrating(true);
    try {
      await hydrate(selectedPreset, controller.signal);
    } catch (error) {
      if (!controller.signal.aborted) setError(error instanceof Error ? error.message : String(error));
    } finally {
      if (migrationController.current === controller) setMigrating(false);
    }
  }

  async function migratePreset(preset: BuildPreset): Promise<BuildPreset | null> {
    migrationController.current?.abort();
    const controller = new AbortController();
    migrationController.current = controller;
    setMigrating(true);
    setError(null);
    try {
      if (!catalog) throw new Error("Current catalog metadata is unavailable; retry after game data finishes loading.");
      // All storage reads are inside this lifecycle, never in a store subscriber.
      const original = JSON.stringify(loadBuildPreset(preset.id));
      const migration = migratePresetRequest(preset.request, catalog);
      const verified = await hydrate({ ...preset, request: migration.request, dataVersion }, controller.signal);
      if (!verified || controller.signal.aborted) return null;
      if (JSON.stringify(loadBuildPreset(preset.id)) !== original) {
        throw new Error("The saved record changed during verification. Load it again; no record was overwritten.");
      }
      const migrated = saveBuildPreset({ ...verified, dataVersion });
      setSelectedId(migrated.id);
      setName(migrated.name);
      refresh();
      pushNotice({ scope: "global", tone: migration.issues.length ? "warning" : "success",
        message: `Migrated ${migrated.name} and verified its saved configurations on current data.${migration.issues.length ? ` Cleared filters: ${migration.issues.join("; ")}.` : ""}` });
      return migrated;
    } catch (error) {
      if (!controller.signal.aborted) setError(error instanceof Error ? error.message : String(error));
      return null;
    } finally {
      if (migrationController.current === controller) setMigrating(false);
    }
  }

  function renameCurrent() {
    if (!selectedId) return;
    try {
      renameBuildPreset(selectedId, name);
      cancelMigration();
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
    try {
      const deletedName = selectedPreset?.name ?? "saved build";
      deleteBuildPreset(selectedId);
      cancelMigration();
      setSelectedId("");
      setDeleteArmedId(null);
      refresh();
      pushNotice({ scope: "global", tone: "success", message: `Deleted ${deletedName}.` });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  function selectPreset(id: string) {
    try {
      const preset = id ? loadBuildPreset(id) : null;
      cancelMigration();
      setSelectedId(id);
      setDeleteArmedId(null);
      setName(preset?.name ?? null);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  async function copyCurrent() {
    const preset = selectedPreset;
    if (!preset) return;
    try {
      await navigator.clipboard.writeText(shareTextForPreset(preset));
      // Confirmed on the button itself, where the player is looking (and to screen readers).
      setCopied(true);
    } catch {
      setError("Could not copy to the clipboard. Check clipboard permission, then retry or use Export instead.");
    }
  }

  function exportCurrent() {
    const preset = selectedPreset;
    if (preset) downloadPresetJson(preset);
  }

  async function importCurrent() {
    try {
      const parsed = parsePresetText(importText);
      const preset = replaceImport ? replaceImportedBuildPreset(parsed) : importBuildPreset(parsed);
      cancelMigration();
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
  let selectedPreset: BuildPreset | null = null;
  let selectedReadError: string | null = null;
  try { selectedPreset = selectedId ? loadBuildPreset(selectedId) : null; }
  catch { selectedReadError = "Saved builds could not be read. Storage access is unavailable; existing data was preserved."; }
  const selectedPresetStale = Boolean(selectedPreset && dataVersion !== "unknown" && selectedPreset.dataVersion !== dataVersion);

  // The library is its own task, so it stays folded under Build Detail until it is wanted
  // (Save build opens it). A library problem opens it, so its alert is never folded away.
  const libraryProblem = libraryError ?? selectedReadError;
  useEffect(() => {
    if (libraryProblem && panel.current) panel.current.open = true;
  }, [libraryProblem]);
  return (
    <details id="saved-builds-panel" className="saved-builds" ref={panel}>
      <summary className="inspector-title">
        <Save size={17} />
        <span>Saved Builds</span>
        {entries.length ? <small className="saved-builds-count">{entries.length}</small> : null}
        <ChevronDown size={15} className="summary-chevron" aria-hidden="true" />
      </summary>
      {libraryError || selectedReadError ? <p className="tone-danger" role="alert">{libraryError || selectedReadError}</p> : null}
      <label>
        Name
        <input value={name} onChange={(event) => { cancelMigration(); setName(event.target.value); }} />
      </label>
      <label>
        Saved
        <select value={selectedId} onChange={(event) => selectPreset(event.target.value)}>
          <option value="">None</option>
          {entries.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name} ({entry.profileId}, {entry.dataVersion === dataVersion ? "current data" : "different data"})
            </option>
          ))}
        </select>
      </label>
      {selectedId ? <small className="saved-build-status" title={presetProvenance(selectedPreset?.dataVersion)}>{presetVersionLabel(selectedPreset?.dataVersion, dataVersion)}</small> : null}
      <div className="inspector-actions stacked">
        <button type="button" onClick={() => saveCurrent()} disabled={!canSave}><Save size={15} />Save new</button>
        <button type="button" onClick={() => saveCurrent(selectedId)} disabled={!canSave || !selectedPreset}><Save size={15} />Update selected</button>
        <button type="button" onClick={() => void loadCurrent()} disabled={!selectedPreset || isMigrating || !canLoad}><Upload size={15} />{isMigrating ? "Verifying..." : selectedPresetStale ? "Load inputs only" : "Load"}</button>
        {selectedPresetStale ? (
          <button type="button" onClick={() => selectedPreset && void migratePreset(selectedPreset)} disabled={isMigrating || !canLoad}>
            <Upload size={15} />{isMigrating ? "Migrating..." : "Migrate data"}
          </button>
        ) : null}
      </div>
      <div className="saved-build-tools" role="group" aria-label="Selected saved build">
        <button type="button" onClick={renameCurrent} disabled={!selectedPreset} aria-label="Rename" title="Rename to the name above"><Pencil size={15} /></button>
        <button
          type="button"
          className={deleteArmedId === selectedId ? "armed" : undefined}
          onClick={deleteCurrent}
          disabled={!selectedPreset}
          aria-label={deleteArmedId === selectedId ? "Confirm Delete" : "Delete"}
          title="Delete the selected saved build"
        >
          <Trash2 size={15} />{deleteArmedId === selectedId ? <span>Confirm delete</span> : null}
        </button>
        <button type="button" onClick={exportCurrent} disabled={!selectedPreset} aria-label="Export" title="Export as a JSON file"><Download size={15} /></button>
        <button type="button" onClick={copyCurrent} disabled={!selectedPreset} aria-label="Copy Share" title="Copy share text to the clipboard">
          {copied ? <Check size={15} className="copy-confirm" aria-hidden="true" /> : <Clipboard size={15} aria-hidden="true" />}
        </button>
        <span className="sr-only" role="status">{copied ? "Copied share text." : ""}</span>
      </div>
      <details className="saved-build-import">
        <summary>Import a build</summary>
        <label>
          Import JSON or Share Text
          <textarea value={importText} onChange={(event) => setImportText(event.target.value)} />
        </label>
        {importPreview?.value ? (
          <div className="import-preview">
            <strong>{importPreview.value.preset.name}</strong>
            <span title={presetProvenance(importPreview.value.preset.dataVersion)}>{presetVersionLabel(importPreview.value.preset.dataVersion, dataVersion)}</span>
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
        ) : importPreview?.error ? <small className="tone-danger">{importPreview.error}</small> : null}
        <button className="clear-locks" type="button" onClick={() => void importCurrent()} disabled={!importPreview?.value || isMigrating}>
          <Upload size={15} />{isMigrating ? "Migrating..." : "Import"}
        </button>
      </details>
      <SavedBuildRecovery onChanged={refresh} revision={entries} />
    </details>
  );
}

function migratePresetRequest(request: OptimizeRequestDto, catalog: CatalogDto): { request: OptimizeRequestDto; issues: string[] } {
  const migrated = activationRequest(request, catalog);
  const issues: string[] = [];
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
  if (!savedVersion) return "Saved data version unknown";
  return savedVersion === currentVersion
    ? "Saved with the current game data"
    : "Stale: saved with different game data. Inputs load; solved rows are discarded.";
}

/** Full snapshot identity, kept out of the visible label for troubleshooting. */
function presetProvenance(savedVersion: string | undefined): string | undefined {
  if (!savedVersion) return undefined;
  const parts = savedVersion.split(":");
  const [profile, schema, dataset, model] = parts.length === 4 ? parts : ["unknown", ...parts];
  return `profile ${profile} · dataset ${dataset} · schema ${schema} · model ${model}`;
}
