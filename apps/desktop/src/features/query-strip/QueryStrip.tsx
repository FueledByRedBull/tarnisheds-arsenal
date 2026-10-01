import { Command as CommandIcon, Play, Redo2, RotateCcw, Undo2 } from "lucide-react";
import { lazy, ReactNode, Suspense, useEffect, useEffectEvent, useMemo, useState } from "react";
import brandMark from "../../assets/brand-mark.png";
import { CommandAction, CommandContext } from "../../lib/commands";
import { fixed1, objectiveLabel, statLockLine } from "../../lib/format";
import { useRequestBudget, useWeaponProfile } from "../../lib/hooks";
import { describeStep, redoQuery, trackQueryHistory, undoQuery, useQueryHistory } from "../../lib/query-history";
import { scadutreeAttackMultiplier } from "../../lib/scadutree";
import { classMeta, derivedLevel, EIGHT_STAT_KEYS, hasCombatStatLocks, optimalStartingClass, startingClassLevel } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { EightStatsDto, OptimizeRequestDto } from "../../lib/types";
import { effectiveWeaponStrength } from "../../lib/weapon-handling";
import { ShortcutsDialog } from "../command-palette/ShortcutsDialog";
import { Popover } from "../shared/Popover";
import { DraftNumberInput } from "./DraftNumberInput";
import {
  ClassEditor,
  groupingLabel,
  LimitsEditor,
  loadoutSummary,
  LoadoutEditor,
  ObjectiveEditor,
  resetWeaponFiltersPatch,
  ResultsEditor,
  ScalingEditor,
  UpgradeEditor,
  weaponPatch,
} from "./QueryEditors";
import { useSearchRunner } from "./useSearchRunner";

const CommandPalette = lazy(() => import("../command-palette/CommandPalette")
  .then((module) => ({ default: module.CommandPalette })));

// Text entry keeps its own undo and typing; buttons, checkboxes and the page do not.
function isTextEntry(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable
    || target.matches("textarea, select, input:not([type=checkbox]):not([type=radio]):not([type=button]):not([type=submit])"));
}

const STAT_FIELDS = [
  ["VIG", "vig"],
  ["MND", "mnd"],
  ["END", "end"],
  ["STR", "strStat"],
  ["DEX", "dex"],
  ["INT", "intStat"],
  ["FAI", "fai"],
  ["ARC", "arc"],
] as const;

// The whole query as one editable line: tokens open their editors, the ribbon below keeps
// every stat visible and editable in place, and Ctrl+K edits anything by typing.
export function QueryStrip({ profile, coverage, onProfileChange }: {
  profile: ReactNode;
  coverage: ReactNode;
  onProfileChange: (profileId: string) => void;
}) {
  const catalog = useDesktopStore((state) => state.catalog);
  const profiles = useDesktopStore((state) => state.profiles);
  const activeWorkspace = useDesktopStore((state) => state.activeWorkspace);
  const request = useDesktopStore((state) => state.request);
  const loadoutSelectionRevision = useDesktopStore((state) => state.loadoutSelectionRevision);
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const applyClass = useDesktopStore((state) => state.applyClass);
  const markResultsStale = useDesktopStore((state) => state.markResultsStale);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const hasRows = useDesktopStore((state) => state.rows.length > 0);
  const hasSelection = useDesktopStore((state) => state.selected !== null);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const isExporting = useDesktopStore((state) => state.isExporting);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const setLockedStatMode = useDesktopStore((state) => state.setLockedStatMode);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  const runner = useSearchRunner();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteLoaded, setPaletteLoaded] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const undoStep = useQueryHistory((state) => state.past.at(-1) ?? null);
  const redoStep = useQueryHistory((state) => state.future.at(-1) ?? null);
  const announcement = useQueryHistory((state) => state.announcement);
  const undoLabel = undoStep ? describeStep(undoStep) : null;
  const redoLabel = redoStep ? describeStep(redoStep) : null;

  const meta = classMeta(catalog, request.className);
  const { budget } = useRequestBudget(catalog, request, lockedStatMode);
  // Mounted once here so a selected weapon's legal affinity is enforced while editors are closed.
  const profileResource = useWeaponProfile(request, patchRequest);
  const weaponProfile = profileResource.profile;
  const effectiveStr = effectiveWeaponStrength(request.strStat, request.twoHanding, weaponProfile);
  const requirementGaps = useMemo(() => {
    const requirements = weaponProfile?.requirements;
    if (!requirements || effectiveStr === null) return null;
    return {
      strStat: Math.max(requirements.strStat - effectiveStr, 0),
      dex: Math.max(requirements.dex - request.dex, 0),
      intStat: Math.max(requirements.intStat - request.intStat, 0),
      fai: Math.max(requirements.fai - request.fai, 0),
      arc: Math.max(requirements.arc - request.arc, 0),
    } as Record<string, number>;
  }, [effectiveStr, request.arc, request.dex, request.fai, request.intStat, weaponProfile]);
  const missingRequirements = requirementGaps ? Object.values(requirementGaps).some((gap) => gap > 0) : false;

  const fixedStats = catalog?.dataManifest.capabilities.classBudget === false;
  const rules = catalog?.dataManifest.rules;
  const separateUpgradeCaps = rules?.separateUpgradeCaps ?? true;
  const scadutreeAvailable = rules?.scadutreeScaling ?? true;
  const standardUpgradeLimit = rules?.standardMaxUpgrade ?? 25;
  const somberUpgradeLimit = rules?.somberMaxUpgrade ?? 10;
  const exactLocksActive = lockedStatMode && hasCombatStatLocks(request);
  const activeMinimums = [request.minStr, request.minDex, request.minInt, request.minFai, request.minArc]
    .filter((value) => value > 0).length;
  const savedCoverageCount = request.filters.entries.filter((entry) => entry.dimension === "coverage").length;
  const analysesAvailable = catalog !== null && hasSelection && !resultsStale && !fixedStats;
  const searchBusy = isSearching || runner.isPreparingSearch;
  const showSearch = activeWorkspace === "rankings" || searchBusy;

  const upgradeSummary = separateUpgradeCaps
    ? `${request.exactUpgrade ? "Exact" : "Up to"} +${request.standardMaxUpgrade} / +${request.somberMaxUpgrade}`
    : `${request.exactUpgrade ? "Exact" : "Up to"} +${request.standardMaxUpgrade}`;
  const scalingSummary = !scadutreeAvailable
    ? "No Scadutree"
    : request.dlcScaling
      ? `Blessing ${request.scadutreeLevel}`
      : "Base game";
  const resultsSummary = `Top ${request.topK}${request.resultGrouping === "automatic" ? "" : `, ${groupingLabel(request.resultGrouping).toLowerCase()}`}`;
  const limitParts = [
    exactLocksActive ? "Stat locks" : null,
    activeMinimums ? `${activeMinimums} minimum${activeMinimums === 1 ? "" : "s"}` : null,
    savedCoverageCount ? "Profile filters" : null,
  ].filter(Boolean);
  const limitsSummary = limitParts.length ? limitParts.join(", ") : "Locks and minimums";
  const loadout = loadoutSummary(catalog, request);
  const classSummary = fixedStats ? "Custom stats" : request.className;

  useEffect(() => trackQueryHistory(), []);

  function startSearch() {
    if (searchBusy || isExporting || !catalog) return;
    setWorkspace("rankings");
    void runner.runSearch();
  }

  const onShortcut = useEffectEvent((event: KeyboardEvent) => {
    const command = (event.ctrlKey || event.metaKey) && !event.altKey;
    const key = event.key.toLowerCase();
    if (command && key === "k") {
      event.preventDefault();
      if (!catalog) return;
      setPaletteLoaded(true);
      setPaletteOpen((open) => !open);
      return;
    }
    // An open modal (palette or shortcut list) owns the keyboard.
    if (event.defaultPrevented || document.querySelector("dialog[open]")) return;
    if (command && (key === "z" || key === "y") && !isTextEntry(event.target)) {
      event.preventDefault();
      if (key === "y" || event.shiftKey) redoQuery();
      else undoQuery();
    } else if (command && event.key === "Enter") {
      event.preventDefault();
      startSearch();
    } else if (event.key === "?" && !command && !isTextEntry(event.target)) {
      event.preventDefault();
      setShortcutsOpen(true);
    }
  });

  useEffect(() => {
    const listener = (event: KeyboardEvent) => onShortcut(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  const commandContext = useMemo<CommandContext | null>(() => catalog ? {
    catalog,
    request,
    lockedStatMode,
    fixedStats,
    isSearching: searchBusy,
    resultsStale,
    hasRows,
    analysesAvailable,
    profiles: profiles.map((entry) => ({
      id: entry.profile.id,
      label: `${entry.profile.displayName} ${entry.profile.modVersion ?? entry.profile.gameVersion}`,
    })),
    undoLabel,
    redoLabel,
  } : null, [analysesAvailable, catalog, fixedStats, hasRows, lockedStatMode, profiles, redoLabel, request, resultsStale, searchBusy, undoLabel]);

  function optimizeClass() {
    const targets: EightStatsDto = request;
    const optimal = optimalStartingClass(catalog, targets, request.className);
    const targetLevel = startingClassLevel(optimal, targets);
    const stats = Object.fromEntries(EIGHT_STAT_KEYS.map((key) => [
      key,
      Math.max(optimal.baseStats[key], targets[key]),
    ])) as unknown as EightStatsDto;
    patchRequest({ className: optimal.name, characterLevel: targetLevel, ...stats });
  }

  function resetStats() {
    if (fixedStats) patchRequest({ vig: 1, mnd: 1, end: 1, strStat: 1, dex: 1, intStat: 1, fai: 1, arc: 1 });
    else applyClass(useDesktopStore.getState().request.className);
  }

  function clearLocks() {
    setLockedStatMode(false);
    patchRequest({ lockStr: null, lockDex: null, lockInt: null, lockFai: null, lockArc: null });
  }

  function runCommand(action: CommandAction) {
    const current = useDesktopStore.getState().request;
    switch (action.kind) {
      case "patch":
        patchRequest(action.patch);
        break;
      case "lock":
        patchRequest(action.patch);
        setLockedStatMode(true);
        break;
      case "class":
        applyClass(action.className);
        break;
      case "weapon":
        patchRequest(weaponPatch(current, action.weaponName));
        runner.markManualWeapon();
        break;
      case "lockedMode":
        setLockedStatMode(action.value);
        break;
      case "clearLocks":
        clearLocks();
        break;
      case "workspace":
        setWorkspace(action.workspace);
        break;
      case "profile":
        onProfileChange(action.profileId);
        break;
      case "search":
        startSearch();
        break;
      case "cancelSearch":
        void runner.cancelSearch();
        break;
      case "optimizeClass":
        optimizeClass();
        break;
      case "resetStats":
        resetStats();
        break;
      case "resetFilters":
        patchRequest(resetWeaponFiltersPatch(current));
        break;
      case "undo":
        undoQuery();
        break;
      case "redo":
        redoQuery();
        break;
      case "shortcuts":
        setShortcutsOpen(true);
        break;
    }
  }

  return (
    <header className="query-strip">
      <div className="strip-line">
        <div className="strip-brand" role="img" aria-label="Tarnished's Arsenal">
          <img src={brandMark} width={30} height={30} alt="" />
          <span aria-hidden="true">Arsenal</span>
        </div>
        {profile}
        <fieldset className="strip-query" disabled={!catalog} aria-busy={!catalog}>
          <legend className="sr-only">Search query</legend>
          <Popover label="Class" trigger={classSummary} triggerLabel={`Class: ${classSummary}`} triggerClassName="strip-token" panelClassName="editor-panel">
            <ClassEditor
              catalog={catalog}
              request={request}
              fixedStats={fixedStats}
              applyClass={applyClass}
              onOptimize={optimizeClass}
              onReset={resetStats}
            />
          </Popover>
          <span className="strip-joiner">maximising</span>
          <Popover label="Objective" trigger={objectiveLabel(request.objective)} triggerLabel={`Objective: ${objectiveLabel(request.objective)}`} triggerClassName="strip-token" panelClassName="editor-panel">
            {(close) => (
              <ObjectiveEditor
                objectives={catalog?.objectiveIds ?? []}
                objective={request.objective}
                patchRequest={patchRequest}
                close={close}
              />
            )}
          </Popover>
          <span className="strip-joiner">with</span>
          <Popover label="Loadout" trigger={<span className="token-text">{loadout}</span>} triggerLabel={`Loadout: ${loadout}`} triggerTitle={loadout} triggerClassName="strip-token loadout-token" panelClassName="editor-panel loadout-panel">
            <LoadoutEditor
              catalog={catalog}
              request={request}
              patchRequest={patchRequest}
              runner={runner}
              loadoutSelectionRevision={loadoutSelectionRevision}
              separateUpgradeCaps={separateUpgradeCaps}
            />
          </Popover>
          <span className="strip-joiner">at</span>
          <Popover label="Upgrades" trigger={upgradeSummary} triggerLabel={`Upgrades: ${upgradeSummary}`} triggerClassName="strip-token" panelClassName="editor-panel">
            <UpgradeEditor
              request={request}
              patchRequest={patchRequest}
              markResultsStale={markResultsStale}
              separateUpgradeCaps={separateUpgradeCaps}
              standardUpgradeLimit={standardUpgradeLimit}
              somberUpgradeLimit={somberUpgradeLimit}
              weaponProfile={weaponProfile}
            />
          </Popover>
          <label className="strip-token strip-toggle" title="Apply the 1.5x effective STR rule when legal">
            <input
              type="checkbox"
              checked={request.twoHanding}
              onChange={(event) => patchRequest({ twoHanding: event.target.checked })}
            />
            Two-handing
          </label>
          <Popover
            label="Scaling"
            trigger={scalingSummary}
            triggerLabel={`Scaling: ${scalingSummary}`}
            triggerTitle={scadutreeAvailable && request.dlcScaling
              ? `Scadutree Blessing ${request.scadutreeLevel}: x${scadutreeAttackMultiplier(true, request.scadutreeLevel).toFixed(2)} damage`
              : undefined}
            triggerClassName="strip-token"
            panelClassName="editor-panel"
          >
            {scadutreeAvailable ? (
              <ScalingEditor request={request} patchRequest={patchRequest} markResultsStale={markResultsStale} />
            ) : (
              <div className="editor-body profile-rule-note" role="note">
                <span>Convergence rule</span>
                <strong>Scadutree Blessing is removed</strong>
                <small>Weapon AR is calculated without Shadow Realm blessing controls.</small>
              </div>
            )}
          </Popover>
          <Popover label="Results" trigger={resultsSummary} triggerLabel={`Results: ${resultsSummary}`} triggerClassName="strip-token" panelClassName="editor-panel">
            <ResultsEditor request={request} patchRequest={patchRequest} markResultsStale={markResultsStale} />
          </Popover>
          <Popover
            label="Limits"
            trigger={limitParts.length ? limitsSummary : <><span aria-hidden="true">+ </span>Limits</>}
            triggerLabel={`Limits: ${limitsSummary}`}
            triggerClassName={`strip-token ${limitParts.length ? "attention" : "quiet"}`}
            panelClassName="editor-panel"
          >
            <LimitsEditor
              request={request}
              patchRequest={patchRequest}
              markResultsStale={markResultsStale}
              fixedStats={fixedStats}
              lockedStatMode={lockedStatMode}
              exactLocksActive={exactLocksActive}
              onClearLocks={clearLocks}
            />
          </Popover>
        </fieldset>
        <div className="strip-actions">
          <div className="strip-history" role="group" aria-label="Query history">
            <button
              type="button"
              className="strip-icon"
              aria-label={undoLabel ? `Undo ${undoLabel}` : "Undo"}
              title={undoLabel ? `Undo ${undoLabel} (Ctrl+Z)` : "Nothing to undo"}
              aria-keyshortcuts="Control+Z"
              disabled={!undoLabel}
              onClick={() => undoQuery()}
            >
              <Undo2 size={15} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="strip-icon"
              aria-label={redoLabel ? `Redo ${redoLabel}` : "Redo"}
              title={redoLabel ? `Redo ${redoLabel} (Ctrl+Y)` : "Nothing to redo"}
              aria-keyshortcuts="Control+Y Control+Shift+Z"
              disabled={!redoLabel}
              onClick={() => redoQuery()}
            >
              <Redo2 size={15} aria-hidden="true" />
            </button>
            <span className="sr-only" role="status">{announcement}</span>
          </div>
          <button
            type="button"
            className="strip-palette"
            aria-label="Edit anything"
            aria-keyshortcuts="Control+K"
            title="Edit anything by typing (Ctrl+K). Press ? for every shortcut."
            disabled={!catalog}
            onClick={() => {
              setPaletteLoaded(true);
              setPaletteOpen(true);
            }}
          >
            <CommandIcon size={15} aria-hidden="true" />
            <span className="strip-palette-label">Edit anything</span>
            <kbd>Ctrl K</kbd>
          </button>
          {showSearch ? (
            <button
              className={`search-button ${searchBusy ? "busy" : ""}`}
              type="button"
              title={searchBusy ? undefined : "Shortcut: Ctrl+Enter"}
              aria-keyshortcuts={searchBusy ? undefined : "Control+Enter"}
              onClick={searchBusy ? runner.cancelSearch : runner.runSearch}
              disabled={isExporting || runner.searchCancellationRequested || (!isSearching && !catalog)}
            >
              {searchBusy ? <RotateCcw size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
              {searchBusy
                ? (runner.searchCancellationRequested ? "Cancelling…" : "Cancel Search")
                : resultsStale ? "Update Results" : "Search"}
            </button>
          ) : null}
        </div>
      </div>

      <fieldset className="strip-ribbon" disabled={!catalog}>
        <legend className="sr-only">Character stats</legend>
        <label className="ribbon-level">
          {fixedStats ? "Stat total" : "Level"}
          <input readOnly value={derivedLevel(catalog, request)} />
        </label>
        {!fixedStats ? (
          <span className="ribbon-readout" title={`Levels above ${request.className}'s base level ${budget.baseLevel}`}>
            <span>Level-ups</span>
            <strong>{budget.levelUps}</strong>
          </span>
        ) : null}
        <span
          className="ribbon-readout"
          title={fixedStats
            ? "Convergence uses the entered combat stats exactly"
            : "Movable STR/DEX/INT/FAI/ARC points after class minimums, fixed VIG/MND/END, and minimum floors"}
        >
          <span>{fixedStats ? "Mode" : "Movable"}</span>
          <strong>{fixedStats ? "Fixed stats" : budget.redistributable}</strong>
        </span>
        <span className="ribbon-divider" aria-hidden="true" />
        <div className="ribbon-stats">
          {STAT_FIELDS.map(([label, key], index) => (
            <label
              key={key}
              className={`ribbon-stat${index === 3 ? " combat-start" : ""}${requirementGaps && (requirementGaps[key] ?? 0) > 0 ? " stat-short" : ""}`}
              title={requirementGaps && (requirementGaps[key] ?? 0) > 0 ? `${requirementGaps[key]} below the selected weapon's requirement` : undefined}
            >
              {label}
              <DraftNumberInput
                min={Math.max(1, meta.baseStats[key as keyof EightStatsDto])}
                max={99}
                value={Number(request[key as keyof OptimizeRequestDto])}
                onDraftChange={markResultsStale}
                onCommit={(value) => patchRequest({ [key]: value } as Partial<OptimizeRequestDto>)}
              />
            </label>
          ))}
        </div>
        <span className="ribbon-divider" aria-hidden="true" />
        <div className="ribbon-status">
          {exactLocksActive ? (
            <span
              className="active-lock-warning"
              role="status"
              title="Changing class or loadout keeps these locks and may make the query incompatible. Clear locks under Limits for automatic stats."
            >
              <strong>Stat locks active:</strong> <span>{statLockLine(request)}</span>
            </span>
          ) : (
            <span className="ribbon-mode">{fixedStats ? "Exact combat stats" : "Stats optimized"}</span>
          )}
          {weaponProfile ? (
            <span className={`requirements-strip ${missingRequirements ? "missing" : ""}`}>
              <span>{missingRequirements ? "Requirements unmet" : "Requirements clear"}</span>
              <strong>
                STR {weaponProfile.requirements.strStat} / DEX {weaponProfile.requirements.dex} /
                INT {weaponProfile.requirements.intStat} / FAI {weaponProfile.requirements.fai} /
                ARC {weaponProfile.requirements.arc}
              </strong>
            </span>
          ) : null}
          {profileResource.status === "loading" ? (
            <span className="requirements-skeleton" role="status" aria-label="Loading weapon requirements">
              <span className="skeleton" />
              <span className="skeleton" />
            </span>
          ) : null}
          {profileResource.status === "error" ? (
            <span className="ribbon-error" role="alert">
              <small>Weapon profile unavailable: {profileResource.error}</small>
              <button type="button" onClick={profileResource.retry}>Retry weapon profile</button>
            </span>
          ) : null}
          {savedCoverageCount ? <span className="ribbon-alert">Profile filters active</span> : null}
        </div>
        <div className="ribbon-end">
          {runner.isPreparingSearch ? <small className="ribbon-checking" role="status">Checking loadout…</small> : isSearching ? (
            <SearchProgressPanel searchStartedAt={runner.searchStartedAt} objective={objectiveLabel(request.objective)} />
          ) : null}
          {coverage}
        </div>
      </fieldset>

      <ShortcutsDialog open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
      {paletteLoaded ? (
        <Suspense fallback={null}>
          <CommandPalette
            open={paletteOpen}
            context={commandContext}
            onRun={runCommand}
            onClose={() => setPaletteOpen(false)}
          />
        </Suspense>
      ) : null}
    </header>
  );
}

function SearchProgressPanel({ searchStartedAt, objective }: { searchStartedAt: number | null; objective: string }) {
  const progress = useDesktopStore((state) => state.progress);
  const [elapsedMs, setElapsedMs] = useState(0);
  const hasProgress = progress !== null;
  useEffect(() => {
    if (hasProgress) return;
    const startedAt = searchStartedAt ?? Date.now();
    const tick = window.setInterval(() => setElapsedMs(Date.now() - startedAt), 200);
    return () => window.clearInterval(tick);
  }, [hasProgress, searchStartedAt]);
  const pct = progress ? Math.min(100, (progress.checked / Math.max(progress.total, 1)) * 100) : null;
  return (
    <div className="progress-strip" role="status">
      <span className="progress-meta">
        <span>{pct === null ? "Starting" : `${fixed1(pct)}%`}</span>
        <strong aria-label={progress ? `${objective} best score` : "Elapsed time"}>
          {progress ? fixed1(progress.bestScore) : formatDuration(elapsedMs)}
        </strong>
        <span>{progress ? `${progress.eligible} covered` : "checking"}</span>
      </span>
      <span className={`search-progress-bar ${pct === null ? "indeterminate" : ""}`}>
        <i style={pct === null ? undefined : { width: `${pct}%` }} />
      </span>
      <small>{formatDuration(progress?.elapsedMs ?? elapsedMs)}</small>
    </div>
  );
}

function formatDuration(ms: number): string {
  const seconds = Math.max(0, ms / 1000);
  return `${seconds.toFixed(seconds < 10 ? 1 : 0)}s`;
}
