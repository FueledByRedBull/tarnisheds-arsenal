import { create } from "zustand";
import { MAX_COMPARE_PINS, readCompareBench, writeCompareBench } from "./compare-bench";
import { activationRequest, catalogVersion, verifyPresetResults } from "./preset-activation";
import { assertRequest, knownRequestFields } from "./presets";
import { isRecord } from "./stored-build";
import {
  AffinityWatchPayloadDto,
  AffinityWatchProgressDto,
  BuildPreset,
  CatalogDto,
  DataManifestDto,
  CompareControls,
  ObjectiveId,
  Notice,
  OptimizeRequestDto,
  PathPreviewDto,
  PathProgressDto,
  PathModeId,
  SearchProgressDto,
  SolvedBuildDto,
  WorkspaceTab,
} from "./types";
import { applyProfileRules, classMeta, CUSTOM_STATS_CLASS, currentStatTotal, EIGHT_STAT_KEYS, hasCombatStatLocks, knownWithoutCatalog, rowFingerprint, stableSignature, usesFixedStats } from "./session";

export interface DesktopState {
  activeWorkspace: WorkspaceTab;
  profiles: DataManifestDto[];
  catalog: CatalogDto | null;
  catalogStatus: "loading" | "ready" | "error";
  catalogError: string | null;
  request: OptimizeRequestDto;
  /** The query each profile last had, restored when the player switches back to it. */
  profileQueries: Record<string, ProfileQuery>;
  loadoutSelectionRevision: number;
  rows: SolvedBuildDto[];
  /** Objective the current rows were ranked by; null when they did not come from a search. */
  rowsObjective: ObjectiveId | null;
  /** Scored setups the search behind the current rows ranked; null when they did not come from one. */
  rowsSetups: number | null;
  /** The ranking a search replaced, so rows can show how far they moved. */
  rankBaseline: { objective: ObjectiveId; rows: SolvedBuildDto[] } | null;
  resultsStale: boolean;
  selected: SolvedBuildDto | null;
  compareTarget: SolvedBuildDto | null;
  restoredCompareTarget: SolvedBuildDto | null;
  compareBench: SolvedBuildDto[];
  selectedFingerprint: string | null;
  lockedStatMode: boolean;
  compareControls: CompareControls;
  pathHorizon: number;
  pathMode: PathModeId;
  affinityHorizon: number;
  notices: Notice[];
  isPathBusy: boolean;
  pathGeneration: number;
  activePathSignature: string | null;
  activePathJobId: string | null;
  pathProgress: PathProgressDto | null;
  pathSignature: string | null;
  paths: PathPreviewDto[];
  isAffinityBusy: boolean;
  affinityGeneration: number;
  activeAffinitySignature: string | null;
  activeAffinityJobId: string | null;
  affinityProgress: AffinityWatchProgressDto | null;
  affinitySignature: string | null;
  affinityPayload: AffinityWatchPayloadDto | null;
  error: string | null;
  isSearching: boolean;
  isExporting: boolean;
  searchGeneration: number;
  activeSearchSignature: string | null;
  activeJobId: string | null;
  progress: SearchProgressDto | null;
  setWorkspace: (workspace: WorkspaceTab) => void;
  setProfiles: (profiles: DataManifestDto[]) => void;
  beginProfileSwitch: (profileId: string) => void;
  setCatalogLoading: () => void;
  setCatalog: (catalog: CatalogDto) => void;
  setCatalogFailure: (message: string) => void;
  patchRequest: (patch: Partial<OptimizeRequestDto>) => void;
  applyClass: (className: string) => void;
  setRows: (rows: SolvedBuildDto[], setups: number) => void;
  selectRow: (row: SolvedBuildDto | null) => void;
  setCompareTarget: (row: SolvedBuildDto | null) => void;
  toggleCompareBench: (row: SolvedBuildDto) => void;
  clearCompareBench: () => void;
  patchCompareControls: (patch: Partial<CompareControls>) => void;
  setPathHorizon: (horizon: number) => void;
  setPathMode: (mode: PathModeId) => void;
  setAffinityHorizon: (horizon: number) => void;
  setLockedStatMode: (lockedStatMode: boolean) => void;
  useRowAsLocks: (row: SolvedBuildDto) => void;
  pushNotice: (notice: Notice) => void;
  setError: (error: string | null) => void;
  setSearching: (isSearching: boolean) => void;
  setExporting: (isExporting: boolean) => void;
  beginSearch: (signature: string) => number;
  setActiveJobId: (activeJobId: string | null) => void;
  setProgress: (progress: SearchProgressDto | null) => void;
  setPathBusy: (isPathBusy: boolean) => void;
  beginPath: (signature: string) => number;
  setActivePathJobId: (activePathJobId: string | null) => void;
  setPathProgress: (pathProgress: PathProgressDto | null) => void;
  setPaths: (paths: PathPreviewDto[], signature: string | null) => void;
  setAffinityBusy: (isAffinityBusy: boolean) => void;
  beginAffinity: (signature: string) => number;
  setActiveAffinityJobId: (activeAffinityJobId: string | null) => void;
  setAffinityProgress: (affinityProgress: AffinityWatchProgressDto | null) => void;
  setAffinityPayload: (affinityPayload: AffinityWatchPayloadDto | null, signature: string | null) => void;
  loadBuildPreset: (preset: BuildPreset, signal?: AbortSignal) => Promise<BuildPreset | null>;
}

export const defaultRequest: OptimizeRequestDto = {
  profileId: "vanilla",
  className: "Samurai",
  characterLevel: 9,
  vig: 12,
  mnd: 11,
  end: 13,
  strStat: 12,
  dex: 15,
  intStat: 9,
  fai: 8,
  arc: 8,
  minStr: 0,
  minDex: 0,
  minInt: 0,
  minFai: 0,
  minArc: 0,
  lockStr: null,
  lockDex: null,
  lockInt: null,
  lockFai: null,
  lockArc: null,
  standardMaxUpgrade: 25,
  somberMaxUpgrade: 10,
  exactUpgrade: false,
  twoHanding: false,
  dlcScaling: false,
  scadutreeLevel: 0,
  weaponName: null,
  affinity: null,
  aowName: null,
  weaponTypeKey: null,
  somberFilter: "all",
  filters: { version: 1, entries: [] },
  resultGrouping: "automatic",
  objective: "max_ar",
  topK: 25,
};

type ProfileQuery = { request: OptimizeRequestDto; lockedStatMode: boolean };

const QUERY_STORAGE_PREFIX = "tarnisheds-arsenal.query.v1.";
const queryStorageKey = (profileId: string) => `${QUERY_STORAGE_PREFIX}${profileId}`;

// A profile's saved query, checked like an imported preset request. A query that fails the
// checks, or names another profile, is removed and reported, never carried across profiles.
function readStoredQuery(profileId: string): { query: ProfileQuery | null; discarded: boolean } {
  let raw: string | null;
  try {
    raw = localStorage.getItem(queryStorageKey(profileId));
  } catch {
    return { query: null, discarded: false };
  }
  if (raw === null) return { query: null, discarded: false };
  try {
    const value: unknown = JSON.parse(raw);
    if (!isRecord(value) || value.version !== 1 || typeof value.lockedStatMode !== "boolean") throw new Error();
    assertRequest(value.request);
    // Vanilla's classes are known before its catalog loads; a mod's own class is checked
    // against its catalog when that loads.
    if (profileId === "vanilla" && !knownWithoutCatalog(value.request.className)) throw new Error();
    if (value.request.profileId !== profileId) throw new Error();
    return { query: { request: knownRequestFields(value.request), lockedStatMode: value.lockedStatMode }, discarded: false };
  } catch {
    try { localStorage.removeItem(queryStorageKey(profileId)); } catch { /* The next save replaces it. */ }
    return { query: null, discarded: true };
  }
}

// The crash screen promises a fresh workspace, and a saved query may be what broke it.
export function clearSavedQueries() {
  try {
    const keys = Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index));
    for (const key of keys) if (key?.startsWith(QUERY_STORAGE_PREFIX)) localStorage.removeItem(key);
  } catch {
    // Storage is unavailable, so nothing saved can be restored either.
  }
}

// Saves the shown profile's query as it changes, until the returned function is called. Only a
// loaded profile's query is written, so a switch in progress never overwrites another one.
export function persistProfileQueries(): () => void {
  return useDesktopStore.subscribe((state, previous) => {
    if (state.catalogStatus !== "ready" || state.catalog?.dataManifest.profile.id !== state.request.profileId) return;
    if (state.request === previous.request && state.lockedStatMode === previous.lockedStatMode && previous.catalogStatus === "ready") return;
    try {
      localStorage.setItem(queryStorageKey(state.request.profileId),
        JSON.stringify({ version: 1, request: state.request, lockedStatMode: state.lockedStatMode }));
    } catch {
      // Persistence is optional; the query stays in memory for this session.
    }
  });
}

const defaultCompareControls: CompareControls = {
  filters: { version: 1, entries: [] },
  weaponName: null,
  aowName: null,
  matchSelectedAow: true,
  includeSmithing: true,
  includeSomber: true,
};

function invalidateAllJobs(state: DesktopState) {
  return {
    isSearching: false,
    searchGeneration: state.searchGeneration + 1,
    activeSearchSignature: null,
    activeJobId: null,
    progress: null,
    ...invalidateAnalysisJobs(state),
  };
}

function invalidateAnalysisJobs(state: DesktopState) {
  return {
    restoredCompareTarget: null,
    ...invalidatePathJob(state),
    ...invalidateAffinityJob(state),
  };
}

// Invalidating a job also discards its results: they belong to the inputs it ran on.
function invalidatePathJob(state: DesktopState) {
  return {
    isPathBusy: false,
    pathGeneration: state.pathGeneration + 1,
    activePathSignature: null,
    activePathJobId: null,
    pathProgress: null,
    paths: [],
    pathSignature: null,
  };
}

function invalidateAffinityJob(state: DesktopState) {
  return {
    isAffinityBusy: false,
    affinityGeneration: state.affinityGeneration + 1,
    activeAffinitySignature: null,
    activeAffinityJobId: null,
    affinityProgress: null,
    affinityPayload: null,
    affinitySignature: null,
  };
}

export const useDesktopStore = create<DesktopState>()((set, get) => ({
  activeWorkspace: "rankings",
  profiles: [],
  catalog: null,
  catalogStatus: "loading",
  catalogError: null,
  notices: [],
  error: null,
  setWorkspace: (activeWorkspace) => set((state) => ({
    activeWorkspace: activeWorkspace !== "rankings" && usesFixedStats(state.catalog, state.request.className)
      ? "rankings" : activeWorkspace,
  })),
  setProfiles: (profiles) => set({ profiles }),
  beginProfileSwitch: (profileId) =>
    set((state) => {
      const rules = state.profiles.find((entry) => entry.profile.id === profileId)?.rules;
      // Profiles never share data, so a query is never carried across; each profile keeps its
      // own and gets it back on return. Results are not kept: they rerun on demand.
      const switching = state.catalog !== null && state.request.profileId !== profileId;
      const profileQueries = switching
        ? { ...state.profileQueries, [state.request.profileId]: { request: state.request, lockedStatMode: state.lockedStatMode } }
        : state.profileQueries;
      // A failed load leaves no catalog; the profile it was left for still gets its query back.
      // Loading the profile already shown is a retry, which keeps the query being edited, unless
      // nothing was edited yet (startup), when the profile's saved query comes back.
      const sameProfile = state.request.profileId === profileId;
      const keepEdited = sameProfile && state.request !== defaultRequest;
      const stored = keepEdited || profileQueries[profileId] ? { query: null, discarded: false } : readStoredQuery(profileId);
      const remembered = keepEdited ? undefined : profileQueries[profileId] ?? stored.query ?? undefined;
      const profileName = (id: string) => state.profiles.find((entry) => entry.profile.id === id)?.profile.displayName ?? id;
      return ({
      ...invalidateAllJobs(state),
      activeWorkspace: "rankings",
      catalog: null,
      catalogStatus: "loading",
      catalogError: null,
      profileQueries,
      lockedStatMode: remembered?.lockedStatMode ?? (sameProfile && state.lockedStatMode),
      loadoutSelectionRevision: state.loadoutSelectionRevision + 1,
      request: remembered ? applyProfileRules(remembered.request, rules)
        : sameProfile ? applyProfileRules(state.request, rules) : applyProfileRules({
        ...state.request,
        profileId,
        lockStr: null, lockDex: null, lockInt: null, lockFai: null, lockArc: null,
        weaponName: null,
        affinity: null,
        aowName: null,
        weaponTypeKey: null,
        filters: { version: 1, entries: [] },
        objective: "max_ar",
      }, rules, true),
      rows: [],
      rowsObjective: null,
      rowsSetups: null,
      rankBaseline: null,
      resultsStale: false,
      selected: null,
      compareTarget: null,
      compareBench: [],
      selectedFingerprint: null,
      compareControls: { ...defaultCompareControls },
      notices: [
        ...(switching || remembered ? [{
          scope: "global" as const,
          tone: "info" as const,
          message: remembered
            ? `Restored your ${profileName(profileId)} query. Search to rank it again.`
            : `Your ${profileName(state.request.profileId)} query is kept for when you switch back.`,
        }] : []),
        ...(stored.discarded ? [{
          scope: "global" as const,
          tone: "warning" as const,
          message: `Your saved ${profileName(profileId)} query could not be read and was discarded.`,
        }] : []),
      ],
      error: null,
      });
    }),
  setCatalogLoading: () => set({ catalogStatus: "loading", catalogError: null }),
  setCatalog: (catalog) => {
    const restoredComparisons = readCompareBench(catalog);
    const recomputing: Notice = { scope: "global", tone: "info", message: "Recomputing your pinned comparison builds on current game data..." };
    set((state) => {
      const classInfo = catalog.classes.find((entry) => entry.name === state.request.className)
        ?? catalog.classes.find((entry) => entry.name === "Samurai")
        ?? catalog.classes[0]
        ?? classMeta(null, "Samurai");
      // Profiles can share a class name with different base stats (Prisoner). Stats carried from
      // another profile's class that sit below this class's minimums restart from this class.
      const belowClass = EIGHT_STAT_KEYS.some((key) => state.request[key] < classInfo.baseStats[key]);
      const resetClass = classInfo.name !== state.request.className || belowClass;
      return {
        catalog,
        catalogStatus: "ready",
        catalogError: null,
        loadoutSelectionRevision: state.loadoutSelectionRevision + 1,
        request: applyProfileRules({
          ...state.request,
          // Without a class budget the level comes from the entered stats; keep the stored level
          // valid instead of copying Custom stats' base level 0.
          ...(resetClass ? {
            className: classInfo.name,
            ...(catalog.dataManifest.capabilities.classBudget ? { characterLevel: classInfo.baseLevel, ...classInfo.baseStats } : {}),
          } : {}),
          profileId: catalog.dataManifest.profile.id,
          objective: catalog.objectiveIds.includes(state.request.objective)
            ? state.request.objective
            : catalog.objectiveIds[0] ?? "max_ar",
        }, catalog.dataManifest.rules),
        compareBench: restoredComparisons.rows,
        notices: [...state.notices, ...restoredComparisons.notices,
          ...(restoredComparisons.staleRows.length ? [recomputing] : [])],
      };
    });
    if (restoredComparisons.staleRows.length) void recomputeStoredPins(catalog, restoredComparisons.staleRows, recomputing);
  },
  setCatalogFailure: (catalogError) => set({ catalogStatus: "error", catalogError }),
  pushNotice: (notice) =>
    set((state) => ({
      notices: [...state.notices.filter((entry) => entry.scope !== notice.scope), notice],
    })),
  setError: (error) => set({ error }),
  request: defaultRequest,
  profileQueries: {},
  loadoutSelectionRevision: 0,
  lockedStatMode: false,
  pathHorizon: 40,
  pathMode: "no_respec",
  affinityHorizon: 40,
  patchRequest: (patch) =>
    set((state) => ({
      ...invalidateAllJobs(state),
      // Re-selecting Automatic replaces pending defaults even when its value is already null.
      loadoutSelectionRevision: state.loadoutSelectionRevision + Number(
        ["weaponName", "affinity", "aowName", "weaponTypeKey", "somberFilter", "filters"]
          .some(key => Object.hasOwn(patch, key)),
      ),
      request: applyProfileRules(
        { ...state.request, ...patch, profileId: state.request.profileId },
        state.catalog?.dataManifest.rules,
      ),
      compareTarget: null,
      resultsStale: state.rows.length > 0,
      notices: state.notices.filter((notice) => notice.scope !== "rankings"),
    })),
  applyClass: (className) =>
    set((state) => {
      // Custom stats keeps the entered stats; its level is their total.
      if (className === CUSTOM_STATS_CLASS) {
        return {
          ...invalidateAllJobs(state),
          request: { ...state.request, className, characterLevel: currentStatTotal(state.request) },
          compareTarget: null,
          resultsStale: state.rows.length > 0,
        };
      }
      const meta = classMeta(state.catalog, className);
      return {
        ...invalidateAllJobs(state),
        request: {
          ...state.request,
          className,
          characterLevel: meta.baseLevel,
          vig: meta.baseStats.vig,
          mnd: meta.baseStats.mnd,
          end: meta.baseStats.end,
          strStat: meta.baseStats.strStat,
          dex: meta.baseStats.dex,
          intStat: meta.baseStats.intStat,
          fai: meta.baseStats.fai,
          arc: meta.baseStats.arc,
        },
        compareTarget: null,
        resultsStale: state.rows.length > 0,
      };
    }),
  setPathHorizon: (pathHorizon) =>
    set((state) => ({
      ...invalidatePathJob(state),
      pathHorizon,
    })),
  setPathMode: (pathMode) =>
    set((state) => ({
      ...invalidatePathJob(state),
      pathMode,
    })),
  setAffinityHorizon: (affinityHorizon) =>
    set((state) => ({
      ...invalidateAffinityJob(state),
      affinityHorizon,
    })),
  setLockedStatMode: (lockedStatMode) =>
    set((state) => ({
      ...invalidateAllJobs(state),
      lockedStatMode,
      resultsStale: state.rows.length > 0,
      compareTarget: null,
    })),
  useRowAsLocks: (row) =>
    set((state) => ({
      ...invalidateAllJobs(state),
      loadoutSelectionRevision: state.loadoutSelectionRevision + 1,
      request: {
        ...state.request,
        weaponName: row.weaponName,
        affinity: row.affinity,
        aowName: row.aowName,
        weaponTypeKey: null,
        filters: { version: 1, entries: [] },
        somberFilter: "all",
        // A profile with one shared upgrade cap (Convergence) sets both caps to the row's level.
        ...(state.catalog?.dataManifest.rules.separateUpgradeCaps === false
          ? { standardMaxUpgrade: row.upgrade, somberMaxUpgrade: row.upgrade }
          : row.isSomber ? { somberMaxUpgrade: row.upgrade } : { standardMaxUpgrade: row.upgrade }),
        exactUpgrade: true,
        lockStr: row.stats.strStat,
        lockDex: row.stats.dex,
        lockInt: row.stats.intStat,
        lockFai: row.stats.fai,
        lockArc: row.stats.arc,
      },
      lockedStatMode: true,
      compareTarget: null,
      selectedFingerprint: rowFingerprint(row),
      resultsStale: state.rows.length > 0,
      notices: [
        ...state.notices,
        { scope: "rankings", tone: "info", message: "Stat locks set from the selected result. Clear them under More options to search freely." },
      ],
    })),
  loadBuildPreset: async (preset, signal) => {
    const state = get();
    const catalog = state.catalog;
    if (!catalog) throw new Error("Current catalog metadata is unavailable. Wait for game data before loading a saved build.");
    if (preset.profileId !== state.request.profileId) throw new Error(`Switch to ${preset.profileId} before loading this build.`);
    const request = activationRequest(knownRequestFields(preset.request), catalog);
    if (signal?.aborted) return null;
    // Rows saved on other game data are recalculated like current ones; old numbers are never shown.
    const stale = preset.dataVersion !== catalogVersion(catalog);
    const candidate = { ...preset, request };
    // A build saved without pins leaves the current pins alone; one with pins replaces them.
    const keepPins = candidate.compareBench.length === 0;
    if (!keepPins) storedPinRecompute?.abort();
    const controller = new AbortController();
    const abort = () => controller.abort();
    signal?.addEventListener("abort", abort, { once: true });
    const verificationNotice: Notice = { scope: "global", tone: "info", message: `Verifying saved build ${preset.name}...` };
    set({
      ...invalidateAllJobs(state),
      loadoutSelectionRevision: state.loadoutSelectionRevision + 1,
      request: applyProfileRules(request, catalog.dataManifest.rules),
      lockedStatMode: hasCombatStatLocks(request), rows: [], rowsObjective: null, rowsSetups: null, rankBaseline: null, resultsStale: false,
      selected: null, compareTarget: null, restoredCompareTarget: null,
      compareControls: { ...defaultCompareControls }, compareBench: keepPins ? state.compareBench : [], selectedFingerprint: null,
      error: null,
      notices: [verificationNotice],
    });
    const context = get();
    const isCurrent = () => {
      const latest = get();
      return !controller.signal.aborted && latest.catalog === catalog && latest.request === context.request
        && latest.searchGeneration === context.searchGeneration && latest.compareBench === context.compareBench
        && latest.selectedFingerprint === context.selectedFingerprint && latest.compareControls === context.compareControls
        && latest.activeWorkspace === context.activeWorkspace;
    };
    const stop = useDesktopStore.subscribe(() => { if (!isCurrent()) controller.abort(); });
    try {
      const verified = await verifyPresetResults(candidate, catalog, controller.signal);
      if (!isCurrent()) return null;
      stop();
      const replacedPins = keepPins || !state.compareBench.length ? ""
        : ` Its pinned builds replaced the ${state.compareBench.length} you had pinned.`;
      const persistenceNotices = keepPins ? [] : writeCompareBench(catalog, verified.compareBench);
      set({ rows: verified.selectedBuild ? [verified.selectedBuild] : [], selected: verified.selectedBuild,
        compareTarget: verified.compareTarget, restoredCompareTarget: verified.compareTarget,
        ...(keepPins ? {} : { compareBench: verified.compareBench }), selectedFingerprint: rowFingerprint(verified.selectedBuild),
        notices: [{ scope: "global", tone: "success",
          message: stale ? `Loaded ${preset.name}; results recomputed on current game data (saved on ${preset.dataVersion}).${replacedPins}`
            : `Loaded ${preset.name}; saved results verified on current data.${replacedPins}` }, ...persistenceNotices] });
      return verified;
    } catch (error) {
      if (isCurrent()) {
        controller.abort();
        set({ notices: [{ scope: "global", tone: "warning", message: "Saved results could not be verified. Inputs are loaded; the original saved record is unchanged." }] });
        throw error;
      }
      return null;
    } finally {
      stop();
      signal?.removeEventListener("abort", abort);
      if (get().notices.includes(verificationNotice)) {
        set(current => ({ notices: current.notices.filter(notice => notice !== verificationNotice) }));
      }
    }
  },
  isExporting: false,
  setExporting: (isExporting) => set({ isExporting }),
  rows: [],
  rowsObjective: null,
  rowsSetups: null,
  rankBaseline: null,
  resultsStale: false,
  selected: null,
  selectedFingerprint: null,
  isSearching: false,
  searchGeneration: 0,
  activeSearchSignature: null,
  activeJobId: null,
  progress: null,
  setRows: (rows, setups) =>
    set((state) => {
      const selected =
        rows.find((row) => rowFingerprint(row) === state.selectedFingerprint) ??
        rows[0] ??
        null;
      return {
        rows,
        rowsObjective: state.request.objective,
        rowsSetups: setups,
        rankBaseline: state.rows.length && state.rowsObjective ? { objective: state.rowsObjective, rows: state.rows } : null,
        resultsStale: false,
        selected,
        selectedFingerprint: rowFingerprint(selected),
      };
    }),
  selectRow: (selected) =>
    set((state) => {
      const selectedFingerprint = rowFingerprint(selected);
      if (selectedFingerprint === state.selectedFingerprint) return { selected };
      return {
        ...invalidateAnalysisJobs(state),
        selected,
        selectedFingerprint,
      };
    }),
  setSearching: (isSearching) => set({ isSearching }),
  beginSearch: (activeSearchSignature) => {
    let generation = 0;
    set((state) => {
      generation = state.searchGeneration + 1;
      return {
        ...invalidateAnalysisJobs(state),
        compareTarget: null,
        isSearching: true,
        resultsStale: state.rows.length > 0,
        searchGeneration: generation,
        activeSearchSignature,
        activeJobId: null,
        progress: null,
      };
    });
    return generation;
  },
  setActiveJobId: (activeJobId) => set({ activeJobId }),
  setProgress: (progress) => set({ progress }),
  compareTarget: null,
  // A saved comparison remains an input while Compare refreshes its analysis.
  restoredCompareTarget: null,
  compareBench: [],
  compareControls: { ...defaultCompareControls },
  // Paths key on the target's content, so re-resolving the same target keeps their run.
  setCompareTarget: (compareTarget) =>
    set((state) => stableSignature(compareTarget) === stableSignature(state.compareTarget) ? state : {
      ...invalidatePathJob(state),
      compareTarget,
    }),
  toggleCompareBench: (row) =>
    set((state) => {
      const fingerprint = rowFingerprint(row);
      const exists = state.compareBench.some((entry) => rowFingerprint(entry) === fingerprint);
      if (!exists && state.compareBench.length >= MAX_COMPARE_PINS) {
        return { notices: [...state.notices, {
          scope: "global", tone: "warning", message: `Compare holds ${MAX_COMPARE_PINS} pinned builds. Unpin one before pinning another.`,
        }] };
      }
      const compareBench = exists
        ? state.compareBench.filter((entry) => rowFingerprint(entry) !== fingerprint)
        : [...state.compareBench, row];
      const notices = [...state.notices, ...writeCompareBench(state.catalog, compareBench)];
      return {
        ...invalidatePathJob(state),
        compareBench,
        notices,
        compareTarget: null,
        restoredCompareTarget: null,
        ...(exists ? {} : { compareControls: { ...defaultCompareControls } }),
      };
    }),
  clearCompareBench: () =>
    set((state) => {
      storedPinRecompute?.abort();
      const notices = [...state.notices, ...writeCompareBench(state.catalog, [])];
      return {
        ...invalidatePathJob(state),
        notices,
        compareBench: [],
        compareTarget: null,
        restoredCompareTarget: null,
      };
    }),
  patchCompareControls: (patch) =>
    set((state) => {
      const compareControls = { ...state.compareControls, ...patch };
      return {
        ...invalidatePathJob(state),
        compareControls,
        compareTarget: null,
        restoredCompareTarget: null,
      };
    }),
  isPathBusy: false,
  pathGeneration: 0,
  activePathSignature: null,
  activePathJobId: null,
  pathProgress: null,
  pathSignature: null,
  paths: [],
  setPathBusy: (isPathBusy) => set({ isPathBusy }),
  beginPath: (activePathSignature) => {
    let generation = 0;
    set((state) => {
      generation = state.pathGeneration + 1;
      return {
        isPathBusy: true,
        error: null,
        notices: state.notices.filter((notice) => notice.scope !== "paths"),
        pathGeneration: generation,
        activePathSignature,
        activePathJobId: null,
        pathProgress: null,
      };
    });
    return generation;
  },
  setActivePathJobId: (activePathJobId) => set({ activePathJobId }),
  setPathProgress: (pathProgress) => set({ pathProgress }),
  setPaths: (paths, pathSignature) => set({ paths, pathSignature }),
  isAffinityBusy: false,
  affinityGeneration: 0,
  activeAffinitySignature: null,
  activeAffinityJobId: null,
  affinityProgress: null,
  affinitySignature: null,
  affinityPayload: null,
  setAffinityBusy: (isAffinityBusy) => set({ isAffinityBusy }),
  beginAffinity: (activeAffinitySignature) => {
    let generation = 0;
    set((state) => {
      generation = state.affinityGeneration + 1;
      return {
        isAffinityBusy: true,
        error: null,
        notices: state.notices.filter((notice) => notice.scope !== "affinity_watch"),
        affinityGeneration: generation,
        activeAffinitySignature,
        activeAffinityJobId: null,
        affinityProgress: null,
      };
    });
    return generation;
  },
  setActiveAffinityJobId: (activeAffinityJobId) => set({ activeAffinityJobId }),
  setAffinityProgress: (affinityProgress) => set({ affinityProgress }),
  setAffinityPayload: (affinityPayload, affinitySignature) =>
    set({ affinityPayload, affinitySignature }),
}));

// Pins stored with other game data are recalculated with their exact equipment, upgrade and
// stats before they are shown. Stored pins carry no query, so the current one supplies class,
// two-handing, blessing and DLC scaling. A catalog change discards the result; pins changed
// meanwhile are kept first and the recalculated ones are appended. Clearing pins or loading a
// saved build that brings its own pins replaces them, so either one cancels the merge.
let storedPinRecompute: AbortController | null = null;
async function recomputeStoredPins(catalog: CatalogDto, staleRows: SolvedBuildDto[], recomputing: Notice) {
  const context = useDesktopStore.getState();
  const controller = new AbortController();
  storedPinRecompute?.abort();
  storedPinRecompute = controller;
  const isCurrent = () => !controller.signal.aborted && useDesktopStore.getState().catalog === catalog;
  const stop = useDesktopStore.subscribe(() => { if (!isCurrent()) controller.abort(); });
  const preset: BuildPreset = { version: 2, id: "", name: "", profileId: catalog.dataManifest.profile.id, request: context.request,
    selectedBuild: null, compareTarget: null, compareBench: [], dataVersion: catalogVersion(catalog), createdAt: "", updatedAt: "" };
  try {
    const rows: SolvedBuildDto[] = [];
    for (const row of staleRows) {
      try {
        rows.push(...(await verifyPresetResults({ ...preset, compareBench: [row] }, catalog, controller.signal)).compareBench);
      } catch {
        if (!isCurrent()) return;
      }
    }
    if (!isCurrent()) return;
    const dropped = staleRows.length - rows.length;
    useDesktopStore.setState((state) => {
      const known = new Set(state.compareBench.map(rowFingerprint));
      const restored = rows.filter((row) => !known.has(rowFingerprint(row)) && known.add(rowFingerprint(row)));
      const compareBench = [...state.compareBench, ...restored].slice(0, MAX_COMPARE_PINS);
      const cut = state.compareBench.length + restored.length - compareBench.length;
      const verb = (count: number) => count === 1 ? "was" : "were";
      const message = "Recomputed your pinned comparison builds on current game data using your current class, two-handing,"
        + " blessing and DLC scaling settings."
        + (dropped ? ` ${dropped} could not be recomputed and ${verb(dropped)} removed.` : "")
        + (cut ? ` ${cut} did not fit the ${MAX_COMPARE_PINS}-pin limit and ${verb(cut)} not restored.` : "");
      return {
        // Compare re-derives its summary target from the new pins; a target restored with a saved build stays.
        ...(compareBench.length > state.compareBench.length && state.compareTarget !== state.restoredCompareTarget
          ? { ...invalidatePathJob(state), compareTarget: state.restoredCompareTarget } : {}),
        compareBench,
        notices: [...state.notices.filter((notice) => notice !== recomputing),
          { scope: "global", tone: dropped || cut ? "warning" : "success", message }, ...writeCompareBench(catalog, compareBench)],
      };
    });
  } finally {
    stop();
    if (storedPinRecompute === controller) storedPinRecompute = null;
    if (useDesktopStore.getState().notices.includes(recomputing)) {
      useDesktopStore.setState((state) => ({ notices: state.notices.filter((notice) => notice !== recomputing) }));
    }
  }
}
