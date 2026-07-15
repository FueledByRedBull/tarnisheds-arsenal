import { Clipboard, Download, GitCompareArrows, LockKeyhole, Pencil, Radar, Route, Save, Target, Trash2, Upload } from "lucide-react";
import { useEffect, useState } from "react";
import { compactNumber, fixed1, metricForObjective, statLine } from "../../lib/format";
import {
  deleteBuildPreset,
  downloadPresetJson,
  importBuildPreset,
  loadBuildPreset,
  parsePresetText,
  renameBuildPreset,
  saveBuildPreset,
  savedBuildIndex,
  shareTextForPreset,
} from "../../lib/presets";
import { budgetSnapshot } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { AowRouteDto, SavedBuildIndexEntryV1, StatusBuildupDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";

export function Inspector() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const request = useDesktopStore((state) => state.request);
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
          </div>
          <div className="metric-grid">
            <Metric label="Score" value={fixed1(metricForObjective(selected, request.objective))} />
            <Metric label="AR" value={compactNumber(selected.ar.total)} />
            <Metric label="Bleed" value={compactNumber(selected.bleedBuildup)} />
            <Metric label="Raw AoW" value={compactNumber(selected.aowFullSequenceDamage)} />
          </div>
          <small className="model-note">AR and skill values are raw model outputs; enemy defense and negation are not applied.</small>
          <div className="detail-block">
            <span>Combat Stats</span>
            <strong>{statLine(selected)}</strong>
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
            <button type="button" onClick={() => setWorkspace("compare")}><GitCompareArrows size={15} />Compare</button>
            <button type="button" onClick={() => setWorkspace("paths")}><Route size={15} />Paths</button>
            <button type="button" onClick={() => setWorkspace("affinity_watch")}><Radar size={15} />Affinity Watch</button>
          </div>
        </>
      ) : (
        <div className="empty-state compact">
          <strong>No build selected</strong>
          <span>Select a ranked row.</span>
        </div>
      )}
      <SavedBuildPanel />
    </aside>
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
  const hydrate = useDesktopStore((state) => state.loadBuildPreset);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const [entries, setEntries] = useState<SavedBuildIndexEntryV1[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("Build Preset");
  const [importText, setImportText] = useState("");
  const [deleteArmedId, setDeleteArmedId] = useState<string | null>(null);
  const dataVersion = catalog
    ? `${catalog.dataManifest.schemaVersion}:${catalog.dataManifest.datasetVersion}:${catalog.dataManifest.modelVersion}`
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

  function saveCurrent() {
    try {
      const preset = saveBuildPreset({ name, request, selectedBuild: selected, compareTarget, dataVersion });
      setSelectedId(preset.id);
      refresh();
      pushNotice({ scope: "global", tone: "success", message: `Saved ${preset.name}.` });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  function loadCurrent() {
    const preset = currentPreset();
    if (!preset) return;
    if (dataVersion !== "unknown" && preset.dataVersion !== dataVersion) {
      hydrate({ ...preset, selectedBuild: null, compareTarget: null });
      pushNotice({
        scope: "global",
        tone: "warning",
        message: `Loaded ${preset.name} inputs only: saved data ${preset.dataVersion} differs from ${dataVersion}. Rerun the search.`,
      });
      return;
    }
    hydrate(preset);
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
    await navigator.clipboard.writeText(shareTextForPreset(preset));
    pushNotice({ scope: "global", tone: "success", message: "Copied share text." });
  }

  function exportCurrent() {
    const preset = currentPreset();
    if (preset) downloadPresetJson(preset);
  }

  function importCurrent() {
    try {
      const preset = importBuildPreset(parsePresetText(importText));
      setSelectedId(preset.id);
      setImportText("");
      refresh();
      pushNotice({ scope: "global", tone: "success", message: `Imported ${preset.name}.` });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }

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
              {entry.name}{entry.dataVersion === dataVersion ? "" : ` [data ${entry.dataVersion}]`}
            </option>
          ))}
        </select>
      </label>
      <div className="inspector-actions stacked">
        <button type="button" onClick={saveCurrent}><Save size={15} />Save</button>
        <button type="button" onClick={loadCurrent} disabled={!selectedId}><Upload size={15} />Load</button>
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
      <button className="clear-locks" type="button" onClick={importCurrent} disabled={!importText.trim()}>
        <Upload size={15} />Import
      </button>
    </div>
  );
}
