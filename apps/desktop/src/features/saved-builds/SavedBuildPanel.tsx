import { Check, ChevronDown, Clipboard, Download, Pencil, Save, Trash2, Upload } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../../lib/format";
import { catalogVersion, migratePresetRequest } from "../../lib/preset-activation";
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
import { buildOptimizeRequest } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { BuildPreset, SavedBuildIndexEntryV1 } from "../../lib/types";
import { SavedBuildRecovery } from "./SavedBuildRecovery";

// The saved-build library: save, load, rename, delete, share, import and migrate stored builds.
// It is its own task, so Build Detail keeps it folded until it is wanted (Save build opens it).
export function SavedBuildPanel() {
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
  const [library, setLibrary] = useState(readLibrary);
  const { entries, error: libraryError } = library;
  const [selectedId, setSelectedId] = useState(library.entries[0]?.id ?? "");
  // Until the player types a name, a save is named after the selected build, as the game
  // names weapons ("Blood Uchigatana +25").
  const [typedName, setName] = useState<string | null>(null);
  const name = typedName ?? (selected ? `${selected.affinity === "Standard" ? "" : `${selected.affinity} `}${selected.weaponName} +${selected.upgrade}` : "Build Preset");
  const [copied, setCopied] = useState(false);
  const panel = useRef<HTMLDetailsElement>(null);
  // Backup and recovery scans every stored record, so it mounts once the library is first opened.
  const [opened, setOpened] = useState(false);
  const [importText, setImportText] = useState("");
  const [deleteArmedId, setDeleteArmedId] = useState<string | null>(null);
  const [replaceImport, setReplaceImport] = useState(false);
  const [importDataMode, setImportDataMode] = useState<"stale" | "migrate">("stale");
  const [isMigrating, setMigrating] = useState(false);
  const migrationController = useRef<AbortController | null>(null);
  const dataVersion = catalog ? catalogVersion(catalog) : "unknown";
  const canSave = catalogStatus === "ready" && catalog?.dataManifest.profile.id === request.profileId;
  // Loading needs the verified catalog; startup and profile switches must finish first.
  const canLoad = catalogStatus === "ready";

  function refresh() {
    const next = readLibrary();
    setLibrary(next);
    setSelectedId(current => next.entries.some(entry => entry.id === current) ? current : next.entries[0]?.id ?? "");
  }

  useEffect(() => () => migrationController.current?.abort(), []);
  // A first Delete click only arms; it lapses so a later click can never confirm it unseen.
  useEffect(() => {
    if (!deleteArmedId) return;
    const timer = window.setTimeout(() => setDeleteArmedId(null), 4000);
    return () => window.clearTimeout(timer);
  }, [deleteArmedId]);
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
      setError(errorMessage(error));
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
      if (!controller.signal.aborted) setError(errorMessage(error));
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
      if (!controller.signal.aborted) setError(errorMessage(error));
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
      setError(errorMessage(error));
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
      setError(errorMessage(error));
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
      setError(errorMessage(error));
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
            : `Imported ${preset.name}, saved on different game data; its results are recomputed when loaded.`,
        });
      }
    } catch (error) {
      setError(errorMessage(error));
    }
  }

  const importPreview = useMemo(() => {
    if (!importText.trim()) return null;
    try {
      return { value: previewPresetImport(importText), error: null };
    } catch (error) {
      return { value: null, error: errorMessage(error) };
    }
  }, [importText]);
  // Read once per selection or library refresh (entries is replaced on every refresh), not on
  // every render of the panel. Only a listed build has a record to read.
  const { selectedPreset, selectedReadError } = useMemo(() => {
    try {
      return { selectedPreset: entries.some((entry) => entry.id === selectedId) ? loadBuildPreset(selectedId) : null, selectedReadError: null };
    } catch {
      return { selectedPreset: null, selectedReadError: "Saved builds could not be read. Storage access is unavailable; existing data was preserved." };
    }
  }, [selectedId, entries]);
  const selectedPresetStale = Boolean(selectedPreset && dataVersion !== "unknown" && selectedPreset.dataVersion !== dataVersion);

  // A library problem opens the panel, so its alert is never folded away.
  const libraryProblem = libraryError ?? selectedReadError;
  useEffect(() => {
    if (libraryProblem && panel.current) panel.current.open = true;
  }, [libraryProblem]);
  return (
    <details id="saved-builds-panel" className="saved-builds" ref={panel} onToggle={(event) => { if (event.currentTarget.open) setOpened(true); }}>
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
        <button type="button" onClick={() => void loadCurrent()} disabled={!selectedPreset || isMigrating || !canLoad}><Upload size={15} />{isMigrating ? "Verifying…" : "Load"}</button>
        {selectedPresetStale ? (
          <button type="button" onClick={() => selectedPreset && void migratePreset(selectedPreset)} disabled={isMigrating || !canLoad}>
            <Upload size={15} />{isMigrating ? "Migrating…" : "Migrate data"}
          </button>
        ) : null}
      </div>
      <div className="saved-build-tools" role="group" aria-label="Selected saved build">
        <button type="button" onClick={renameCurrent} disabled={!selectedPreset} aria-label="Rename" title="Rename to the name above"><Pencil size={15} /></button>
        <button
          type="button"
          className={deleteArmedId === selectedId ? "armed" : undefined}
          onClick={deleteCurrent}
          onBlur={() => setDeleteArmedId(null)}
          disabled={!selectedPreset}
          aria-label={deleteArmedId === selectedId ? "Confirm Delete" : "Delete"}
          title="Delete the selected saved build"
        >
          <Trash2 size={15} />{deleteArmedId === selectedId ? <span>Confirm delete</span> : null}
        </button>
        <button type="button" onClick={exportCurrent} disabled={!selectedPreset} aria-label="Export" title="Export as a JSON file"><Download size={15} /></button>
        <button type="button" onClick={() => void copyCurrent()} disabled={!selectedPreset} aria-label="Copy Share" title="Copy share text to the clipboard">
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
          <Upload size={15} />{isMigrating ? "Migrating…" : "Import"}
        </button>
      </details>
      {opened ? <SavedBuildRecovery onChanged={refresh} revision={entries} /> : null}
    </details>
  );
}

function readLibrary(): { entries: SavedBuildIndexEntryV1[]; error: string | null } {
  try {
    return { entries: savedBuildIndex().builds, error: null };
  } catch (error) {
    return { entries: [], error: errorMessage(error) };
  }
}

function presetVersionLabel(savedVersion: string | undefined, currentVersion: string): string {
  if (!savedVersion) return "Saved data version unknown";
  return savedVersion === currentVersion
    ? "Saved with the current game data"
    : "Stale: saved with different game data. Results are recomputed on current data when loaded.";
}

/** Full snapshot identity, kept out of the visible label for troubleshooting. */
function presetProvenance(savedVersion: string | undefined): string | undefined {
  if (!savedVersion) return undefined;
  const parts = savedVersion.split(":");
  const [profile, schema, dataset, model] = parts.length === 4 ? parts : ["unknown", ...parts];
  return `profile ${profile} · dataset ${dataset} · schema ${schema} · model ${model}`;
}
