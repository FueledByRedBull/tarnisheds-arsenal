import { create, type StateCreator } from "zustand";
import {
  AffinityWatchPayloadDto,
  AffinityWatchProgressDto,
  BuildPreset,
  CatalogDto,
  DataManifestDto,
  CompareControls,
  Notice,
  OptimizeRequestDto,
  PathPreviewDto,
  PathProgressDto,
  PathModeId,
  SearchProgressDto,
  SolvedBuildDto,
  WorkspaceTab,
} from "./types";
import { applyProfileRules, classMeta, normalizeOptimizeRequest, rowFingerprint } from "./session";

export interface DesktopState {
  activeWorkspace: WorkspaceTab;
  profiles: DataManifestDto[];
  catalog: CatalogDto | null;
  catalogStatus: "loading" | "ready" | "error";
  catalogError: string | null;
  request: OptimizeRequestDto;
  rows: SolvedBuildDto[];
  resultsStale: boolean;
  selected: SolvedBuildDto | null;
  compareTarget: SolvedBuildDto | null;
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
  setRows: (rows: SolvedBuildDto[]) => void;
  markResultsStale: () => void;
  clearResults: (message?: string) => void;
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
  setNotices: (notices: Notice[]) => void;
  pushNotice: (notice: Notice) => void;
  setError: (error: string | null) => void;
  setSearching: (isSearching: boolean) => void;
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
  loadBuildPreset: (preset: BuildPreset) => void;
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
  budgetMode: "target_level",
  offensivePointBudget: 0,
  objective: "max_ar",
  topK: 25,
};

function compareBenchKey(profileId: string): string {
  return `tarnisheds-arsenal.compareBench.v1.${profileId}`;
}

function readCompareBench(catalog: CatalogDto): SolvedBuildDto[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(compareBenchKey(catalog.dataManifest.profile.id));
    if (!raw) return [];
    const value = JSON.parse(raw) as { version?: unknown; datasetVersion?: unknown; rows?: unknown };
    if (value.version !== 1 || value.datasetVersion !== catalog.dataManifest.datasetVersion || !Array.isArray(value.rows)) {
      return [];
    }
    return value.rows.filter(isStoredBuild).slice(0, 8);
  } catch {
    return [];
  }
}

function isStoredBuild(row: unknown): row is SolvedBuildDto {
  if (typeof row !== "object" || row === null) return false;
  const build = row as Partial<SolvedBuildDto>;
  return typeof build.weaponId === "number"
    && typeof build.weaponName === "string"
    && typeof build.affinity === "string"
    && typeof build.upgrade === "number"
    && typeof build.score === "number"
    && typeof build.stats === "object" && build.stats !== null
    && [build.stats.strStat, build.stats.dex, build.stats.intStat, build.stats.fai, build.stats.arc].every(Number.isFinite)
    && typeof build.ar === "object" && build.ar !== null
    && Number.isFinite(build.ar.total);
}

function writeCompareBench(catalog: CatalogDto | null, rows: SolvedBuildDto[]) {
  if (!catalog || typeof localStorage === "undefined") return;
  localStorage.setItem(compareBenchKey(catalog.dataManifest.profile.id), JSON.stringify({
    version: 1,
    datasetVersion: catalog.dataManifest.datasetVersion,
    rows,
  }));
}

type DesktopSlice<T> = StateCreator<DesktopState, [], [], T>;

type UiSlice = Pick<
  DesktopState,
  | "activeWorkspace"
  | "profiles"
  | "catalog"
  | "catalogStatus"
  | "catalogError"
  | "notices"
  | "error"
  | "setWorkspace"
  | "setProfiles"
  | "beginProfileSwitch"
  | "setCatalogLoading"
  | "setCatalog"
  | "setCatalogFailure"
  | "setNotices"
  | "pushNotice"
  | "setError"
>;

type RequestSlice = Pick<
  DesktopState,
  | "request"
  | "lockedStatMode"
  | "pathHorizon"
  | "pathMode"
  | "affinityHorizon"
  | "patchRequest"
  | "applyClass"
  | "setPathHorizon"
  | "setPathMode"
  | "setAffinityHorizon"
  | "setLockedStatMode"
  | "useRowAsLocks"
  | "loadBuildPreset"
>;

type SearchSlice = Pick<
  DesktopState,
  | "rows"
  | "resultsStale"
  | "selected"
  | "selectedFingerprint"
  | "isSearching"
  | "searchGeneration"
  | "activeSearchSignature"
  | "activeJobId"
  | "progress"
  | "setRows"
  | "markResultsStale"
  | "clearResults"
  | "selectRow"
  | "setSearching"
  | "beginSearch"
  | "setActiveJobId"
  | "setProgress"
>;

type CompareSlice = Pick<
  DesktopState,
  | "compareTarget"
  | "compareBench"
  | "compareControls"
  | "setCompareTarget"
  | "toggleCompareBench"
  | "clearCompareBench"
  | "patchCompareControls"
>;

type PathSlice = Pick<
  DesktopState,
  | "isPathBusy"
  | "pathGeneration"
  | "activePathSignature"
  | "activePathJobId"
  | "pathProgress"
  | "pathSignature"
  | "paths"
  | "setPathBusy"
  | "beginPath"
  | "setActivePathJobId"
  | "setPathProgress"
  | "setPaths"
>;

type AffinitySlice = Pick<
  DesktopState,
  | "isAffinityBusy"
  | "affinityGeneration"
  | "activeAffinitySignature"
  | "activeAffinityJobId"
  | "affinityProgress"
  | "affinitySignature"
  | "affinityPayload"
  | "setAffinityBusy"
  | "beginAffinity"
  | "setActiveAffinityJobId"
  | "setAffinityProgress"
  | "setAffinityPayload"
>;

function invalidateAllJobs(state: DesktopState) {
  return {
    isSearching: false,
    searchGeneration: state.searchGeneration + 1,
    activeSearchSignature: null,
    activeJobId: null,
    progress: null,
    isPathBusy: false,
    pathGeneration: state.pathGeneration + 1,
    activePathSignature: null,
    activePathJobId: null,
    pathProgress: null,
    isAffinityBusy: false,
    affinityGeneration: state.affinityGeneration + 1,
    activeAffinitySignature: null,
    activeAffinityJobId: null,
    affinityProgress: null,
  };
}

function invalidateAnalysisJobs(state: DesktopState) {
  return {
    isPathBusy: false,
    pathGeneration: state.pathGeneration + 1,
    activePathSignature: null,
    activePathJobId: null,
    pathProgress: null,
    isAffinityBusy: false,
    affinityGeneration: state.affinityGeneration + 1,
    activeAffinitySignature: null,
    activeAffinityJobId: null,
    affinityProgress: null,
  };
}

function invalidatePathJob(state: DesktopState) {
  return {
    isPathBusy: false,
    pathGeneration: state.pathGeneration + 1,
    activePathSignature: null,
    activePathJobId: null,
    pathProgress: null,
  };
}

const createUiSlice: DesktopSlice<UiSlice> = (set) => ({
  activeWorkspace: "rankings",
  profiles: [],
  catalog: null,
  catalogStatus: "loading",
  catalogError: null,
  notices: [],
  error: null,
  setWorkspace: (activeWorkspace) => set({ activeWorkspace }),
  setProfiles: (profiles) => set({ profiles }),
  beginProfileSwitch: (profileId) =>
    set((state) => {
      const rules = state.profiles.find((entry) => entry.profile.id === profileId)?.rules;
      return ({
      ...invalidateAllJobs(state),
      activeWorkspace: "rankings",
      catalog: null,
      catalogStatus: "loading",
      catalogError: null,
      request: applyProfileRules({
        ...state.request,
        profileId,
        weaponName: null,
        affinity: null,
        aowName: null,
        weaponTypeKey: null,
        objective: "max_ar",
      }, rules, true),
      rows: [],
      resultsStale: false,
      selected: null,
      compareTarget: null,
      compareBench: [],
      selectedFingerprint: null,
      compareControls: {
        weaponTypeKey: null,
        weaponName: null,
        affinity: null,
        aowName: null,
        matchSelectedAow: true,
      },
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: [],
      error: null,
      });
    }),
  setCatalogLoading: () => set({ catalogStatus: "loading", catalogError: null }),
  setCatalog: (catalog) => set((state) => {
    const classInfo = catalog.classes.find((entry) => entry.name === state.request.className)
      ?? catalog.classes.find((entry) => entry.name === "Samurai")
      ?? classMeta(null, "Samurai");
    const resetClass = classInfo.name !== state.request.className;
    return {
      catalog,
      catalogStatus: "ready",
      catalogError: null,
      request: applyProfileRules({
        ...state.request,
        ...(resetClass ? {
          className: classInfo.name,
          characterLevel: classInfo.baseLevel,
          offensivePointBudget: 0,
          ...classInfo.baseStats,
        } : {}),
        profileId: catalog.dataManifest.profile.id,
        objective: catalog.objectiveIds.includes(state.request.objective)
          ? state.request.objective
          : catalog.objectiveIds[0] ?? "max_ar",
      }, catalog.dataManifest.rules),
      compareBench: readCompareBench(catalog),
    };
  }),
  setCatalogFailure: (catalogError) => set({ catalogStatus: "error", catalogError }),
  setNotices: (notices) => set({ notices }),
  pushNotice: (notice) =>
    set((state) => ({
      notices: [...state.notices.filter((entry) => entry.scope !== notice.scope), notice],
    })),
  setError: (error) => set({ error }),
});

const createRequestSlice: DesktopSlice<RequestSlice> = (set) => ({
  request: defaultRequest,
  lockedStatMode: false,
  pathHorizon: 40,
  pathMode: "no_respec",
  affinityHorizon: 40,
  patchRequest: (patch) =>
    set((state) => ({
      ...invalidateAllJobs(state),
      request: applyProfileRules(
        { ...state.request, ...patch, profileId: state.request.profileId },
        state.catalog?.dataManifest.rules,
      ),
      compareTarget: null,
      resultsStale: state.rows.length > 0,
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: state.notices.filter((notice) => notice.scope !== "rankings"),
    })),
  applyClass: (className) =>
    set((state) => {
      const meta = classMeta(state.catalog, className);
      return {
        ...invalidateAllJobs(state),
        request: {
          ...state.request,
          className,
          characterLevel: meta.baseLevel,
          offensivePointBudget: 0,
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
        paths: [],
        pathSignature: null,
        affinityPayload: null,
        affinitySignature: null,
      };
    }),
  setPathHorizon: (pathHorizon) =>
    set((state) => ({
      ...invalidatePathJob(state),
      pathHorizon,
      paths: [],
      pathSignature: null,
    })),
  setPathMode: (pathMode) =>
    set((state) => ({
      ...invalidatePathJob(state),
      pathMode,
      paths: [],
      pathSignature: null,
    })),
  setAffinityHorizon: (affinityHorizon) =>
    set((state) => ({
      isAffinityBusy: false,
      affinityGeneration: state.affinityGeneration + 1,
      activeAffinitySignature: null,
      activeAffinityJobId: null,
      affinityProgress: null,
      affinityHorizon,
      affinityPayload: null,
      affinitySignature: null,
    })),
  setLockedStatMode: (lockedStatMode) =>
    set((state) => ({
      ...invalidateAllJobs(state),
      lockedStatMode,
      resultsStale: state.rows.length > 0,
      compareTarget: null,
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
    })),
  useRowAsLocks: (row) =>
    set((state) => ({
      ...invalidateAllJobs(state),
      request: {
        ...state.request,
        weaponName: row.weaponName,
        affinity: row.affinity,
        aowName: row.aowName,
        weaponTypeKey: null,
        somberFilter: "all",
        standardMaxUpgrade: row.isSomber ? state.request.standardMaxUpgrade : row.upgrade,
        somberMaxUpgrade: row.isSomber ? row.upgrade : state.request.somberMaxUpgrade,
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
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: [
        ...state.notices,
        { scope: "rankings", tone: "info", message: "Locked selected result; rerun search for exact locked stats." },
      ],
    })),
  loadBuildPreset: (preset) =>
    set((state) => {
      if (preset.profileId !== state.request.profileId) {
        return {
          notices: [{
            scope: "global",
            tone: "warning",
            message: `${preset.name} belongs to ${preset.profileId}. Switch profiles before loading it.`,
          }],
        };
      }
      writeCompareBench(state.catalog, preset.compareBench);
      return {
        ...invalidateAllJobs(state),
        request: applyProfileRules(
          normalizeOptimizeRequest(preset.request, state.request),
          state.catalog?.dataManifest.rules,
        ),
        lockedStatMode: preset.request.lockStr !== null,
        rows: preset.selectedBuild ? [preset.selectedBuild] : [],
        resultsStale: false,
        selected: preset.selectedBuild,
        compareTarget: preset.compareTarget,
        compareBench: preset.compareBench,
        selectedFingerprint: rowFingerprint(preset.selectedBuild),
        paths: [],
        pathSignature: null,
        affinityPayload: null,
        affinitySignature: null,
        notices: [{ scope: "global", tone: "success", message: `Loaded ${preset.name}.` }],
      };
    }),
});

const createSearchSlice: DesktopSlice<SearchSlice> = (set) => ({
  rows: [],
  resultsStale: false,
  selected: null,
  selectedFingerprint: null,
  isSearching: false,
  searchGeneration: 0,
  activeSearchSignature: null,
  activeJobId: null,
  progress: null,
  setRows: (rows) =>
    set((state) => {
      const selected =
        rows.find((row) => rowFingerprint(row) === state.selectedFingerprint) ??
        rows[0] ??
        null;
      return {
        rows,
        resultsStale: false,
        selected,
        selectedFingerprint: rowFingerprint(selected),
      };
    }),
  markResultsStale: () =>
    set((state) => ({ resultsStale: state.rows.length > 0 })),
  clearResults: (message) =>
    set((state) => ({
      rows: [],
      resultsStale: false,
      selected: null,
      compareTarget: null,
      selectedFingerprint: null,
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: message
        ? [...state.notices, { scope: "rankings", tone: "warning", message }]
        : state.notices,
    })),
  selectRow: (selected) =>
    set((state) => ({
      ...invalidateAnalysisJobs(state),
      selected,
      selectedFingerprint: rowFingerprint(selected),
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
    })),
  setSearching: (isSearching) => set({ isSearching }),
  beginSearch: (activeSearchSignature) => {
    let generation = 0;
    set((state) => {
      generation = state.searchGeneration + 1;
      return {
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
});

const createCompareSlice: DesktopSlice<CompareSlice> = (set) => ({
  compareTarget: null,
  compareBench: [],
  compareControls: {
    weaponTypeKey: null,
    weaponName: null,
    affinity: null,
    aowName: null,
    matchSelectedAow: true,
  },
  setCompareTarget: (compareTarget) =>
    set((state) => ({
      ...invalidatePathJob(state),
      compareTarget,
      paths: [],
      pathSignature: null,
    })),
  toggleCompareBench: (row) =>
    set((state) => {
      const fingerprint = rowFingerprint(row);
      const exists = state.compareBench.some((entry) => rowFingerprint(entry) === fingerprint);
      const compareBench = exists
        ? state.compareBench.filter((entry) => rowFingerprint(entry) !== fingerprint)
        : [...state.compareBench, row].slice(-8);
      writeCompareBench(state.catalog, compareBench);
      return { compareBench };
    }),
  clearCompareBench: () =>
    set((state) => {
      writeCompareBench(state.catalog, []);
      return { compareBench: [] };
    }),
  patchCompareControls: (patch) =>
    set((state) => ({
      ...invalidatePathJob(state),
      compareControls: { ...state.compareControls, ...patch },
      compareTarget: null,
      paths: [],
      pathSignature: null,
    })),
});

const createPathSlice: DesktopSlice<PathSlice> = (set) => ({
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
});

const createAffinitySlice: DesktopSlice<AffinitySlice> = (set) => ({
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
});

export const useDesktopStore = create<DesktopState>()((...args) => ({
  ...createUiSlice(...args),
  ...createRequestSlice(...args),
  ...createSearchSlice(...args),
  ...createCompareSlice(...args),
  ...createPathSlice(...args),
  ...createAffinitySlice(...args),
}));
