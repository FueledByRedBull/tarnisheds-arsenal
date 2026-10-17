import { LockKeyhole, Pin, Save, Target } from "lucide-react";
import { useEffect, useRef } from "react";
import { useWeaponProfileResource } from "../../lib/hooks";
import { aowModelSupported, compactNumber, fixed1, hasAowDamage, metricForObjective, objectiveLabel, statLine, statLockLine } from "../../lib/format";
import { budgetSnapshot, buildOptimizeRequest, hasCombatStatLocks, respecChanges, rowFingerprint, usesFixedStats } from "../../lib/session";
import { EASE_OUT, reducedMotion } from "../../lib/motion";
import { useDesktopStore } from "../../lib/state";
import { AowRouteDto, StatusBuildupDto, WeaponProfileDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";
import { ScalingTokens, StatusTokens } from "../shared/BuildMetricTokens";
import packageInfo from "../../../package.json";
import { explainBuild, leadOver } from "../../lib/build-explanation";
import { ReproductionReport } from "../shared/ReproductionReport";
import { SavedBuildPanel } from "../saved-builds/SavedBuildPanel";

export function Inspector() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const rows = useDesktopStore((state) => state.rows);
  const rowsSetups = useDesktopStore((state) => state.rowsSetups);
  const request = useDesktopStore((state) => state.request);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const applyRowLocks = useDesktopStore((state) => state.useRowAsLocks);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const toggleCompareBench = useDesktopStore((state) => state.toggleCompareBench);
  const isExporting = useDesktopStore((state) => state.isExporting);
  const snapshot = budgetSnapshot(catalog, request);
  const fixedStats = usesFixedStats(catalog, request.className);
  const aowSupported = aowModelSupported(catalog?.dataManifest);
  const aowAvailable = selected && hasAowDamage(selected, aowSupported);
  // Skill damage shows only when these results model it; Rankings says once when they don't.
  const aowTile = aowAvailable || rows.some((row) => hasAowDamage(row, aowSupported));
  const selectedMetric = selected ? metricForObjective(selected, request.objective, aowSupported) : null;
  const pinned = Boolean(selected && compareBench.some((entry) => rowFingerprint(entry) === rowFingerprint(selected)));
  const rank = selected ? rows.findIndex((row) => rowFingerprint(row) === rowFingerprint(selected)) : -1;
  const leaderMetric = rows[0] ? metricForObjective(rows[0], request.objective, aowSupported) : null;
  const behindLeader = rank > 0 && leaderMetric !== null && selectedMetric !== null ? leaderMetric - selectedMetric : null;
  // The trust line: how far #1 leads #2, and the stat changes it asks of the entered build.
  const runnerUp = rank === 0 ? rows[1] ?? null : null;
  const lead = selected && runnerUp ? leadOver(selected, runnerUp, request.objective, aowSupported) : null;
  const statChanges = selected && !fixedStats && !resultsStale ? respecChanges(selected, request) : null;
  const modelWarnings = [...new Set(selected?.aowRoute?.actions.flatMap(
    (action) => action.hits.flatMap((hit) => hit.warnings),
  ) ?? [])];
  // A new selection updates the panel in place and fades it in, instead of rebuilding it.
  const detail = useRef<HTMLDivElement>(null);
  const selectedKey = rowFingerprint(selected);
  useEffect(() => {
    if (!selectedKey) return;
    // Reduced motion keeps the fade and drops the rise.
    detail.current?.animate(
      reducedMotion()
        ? [{ opacity: 0.35 }, { opacity: 1 }]
        : [{ opacity: 0.35, transform: "translateY(4px)" }, { opacity: 1, transform: "none" }],
      { duration: 200, easing: EASE_OUT },
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
          {modelWarnings.length ? (
            <div className="selection-notes">
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
            {aowTile ? <Metric label="Raw AoW" value={aowAvailable ? compactNumber(selected.aowFullSequenceDamage) : null} /> : null}
          </div>
          {/* A loaded saved build is alone in the list without having been ranked against anything. */}
          {rank >= 0 && !resultsStale && rowsSetups !== null ? (
            <p className="rank-context">
              Rank <strong>{rank + 1}</strong> of {rows.length}
              {rank === 0
                ? lead === null ? " · best for this query"
                  : lead < 0.05 ? " · less than 0.05 ahead of #2" : ` · ${fixed1(lead)} ahead of #2`
                : behindLeader === null ? ""
                  : behindLeader < 0.05 ? " · less than 0.05 behind #1" : ` · ${fixed1(behindLeader)} behind #1`}
            </p>
          ) : null}
          {rank >= 0 && !resultsStale && rowsSetups !== null ? (
            <p className="rank-proof">
              Compared exactly against {rowsSetups.toLocaleString()} scored {rowsSetups === 1 ? "setup" : "setups"}.
            </p>
          ) : null}
          {statChanges ? (
            <div className="detail-block stat-changes">
              <span>Stat changes</span>
              {/* Each change wraps as a unit, never between its arrow and value. */}
              <strong>{statChanges.length ? statChanges.map((change) => change.replaceAll(" ", "\u00a0")).join(", ") : "None: uses your stats as entered"}</strong>
            </div>
          ) : null}
          <div className="inspector-actions stacked">
            <button type="button" onClick={() => void lockSelected()} disabled={isExporting}><LockKeyhole size={15} />Lock stats to this build</button>
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
            {explainBuild(selected, buildOptimizeRequest(catalog, request, lockedStatMode), aowSupported, runnerUp).map(line => <p key={line}>{line}</p>)}
          </details> : null}
        </div>
      ) : (
        <div className="empty-state compact">
          <strong>No build selected</strong>
          <span>Search, then pick a ranked row to see why it wins.</span>
        </div>
      )}
      {/* Reference detail: weapon poise, the stat budget and locks (also shown in the strip),
          and what the model covers. Folded, so the answer above stays short. */}
      <details className="model-coverage more-details">
        <summary>More details</summary>
        {selected ? <WeaponPoiseDetails profile={weaponProfile} route={aowAvailable ? selected.aowRoute : null} twoHanding={request.twoHanding} /> : null}
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
        <ModelCoverage />
      </details>
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
  const aowModelUnavailable = catalog && !aowModelSupported(catalog.dataManifest);
  const profileRules = catalog?.dataManifest.rules;
  const objectiveWarning = aowModelUnavailable
    ? "Weapon AR, status, passives, affinities, and AoW compatibility are modeled. This profile's AoW hit and route damage is not mapped, so those objectives are unavailable."
    : request.objective === "max_ar_plus_bleed"
      ? "Buildup is modeled, but enemy resistance growth and proc explosion damage are not."
      : request.objective === "aow_first_hit" || request.objective === "aow_full_sequence"
        ? "Legal route damage, status, buff timing, physical attribute, and stamina are reported. Stamina is not optimized and unsupported effects remain warnings."
        : "Attack rating is calculated before enemy defense and negation.";
  return (
    <div className="model-coverage-notes">
      <strong>Model coverage and assumptions</strong>
      <p>{objectiveWarning}</p>
      {aowModelUnavailable ? null : <p>Profile capabilities do not guarantee every skill is modeled; a skill that isn&apos;t shows no skill damage.</p>}
      <p>Scored setups count stat spreads that differ only in stats a setup doesn't use once.</p>
      <p>Temporary buff stacking is not a universal layer. Values are raw model outputs, not expected damage against a specific enemy.</p>
      <small>
        App {packageInfo.version} · {catalog?.dataManifest.profile.displayName ?? "unknown profile"} · dataset {catalog?.dataManifest.datasetVersion ?? "unknown"} · schema {catalog?.dataManifest.schemaVersion ?? "unknown"} · model {catalog?.dataManifest.modelVersion ?? "unknown"}
      </small>
      <small>
        {request.twoHanding ? "Two-handed" : "One-handed"} · {profileRules?.scadutreeScaling
          ? request.dlcScaling ? `Scadutree ${request.scadutreeLevel}` : "DLC scaling off"
          : "Scadutree unavailable"} · upgrade {request.exactUpgrade ? "exact" : "open range"}
      </small>
    </div>
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
  // Without mapped moves the attack values are unknown, not zero, so they are not shown as "None".
  const poiseEntries: [string, string | number[]][] = profile.moveCount ? [
    ["R1", poise.light],
    ["R2", poise.heavy],
    ["Charged R2", poise.chargedHeavy],
    ["Jumping R1", poise.jumpingLight],
    ["Jumping R2", poise.jumpingHeavy],
  ] : [];
  if (aowPoise.length) poiseEntries.push(["AoW", aowPoise]);
  return (
    <div className="detail-block weapon-poise-detail">
      <span>Weapon data</span>
      <strong>Weight {fixed1(profile.weight)} · {profile.moveCount} mapped poise moves · {isTwoHanded ? "2H" : "1H"}</strong>
      <small className="poise-damage-note">
        {profile.moveCount ? "PvE stance / poise damage" : "Attack poise damage isn't mapped for this weapon."}
      </small>
      {poiseEntries.length ? (
        <div>
          {poiseEntries
            .map(([label, value]) => <small key={label}><b>{label}</b>{formatPoiseDamage(value)}</small>)}
        </div>
      ) : null}
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
