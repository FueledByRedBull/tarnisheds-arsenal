import { create } from "zustand";
import {
  AffinityWatchPayloadDto,
  AffinityWatchProgressDto,
  CatalogDto,
  CompareControls,
  Notice,
  ObjectiveId,
  OptimizeRequestDto,
  PathPreviewDto,
  PathProgressDto,
  SearchEstimateDto,
  SearchProgressDto,
  SolvedBuildDto,
  WorkspaceTab,
} from "./types";
import { classMeta, rowFingerprint } from "./session";

export interface DesktopState {
  activeWorkspace: WorkspaceTab;
  catalog: CatalogDto | null;
  request: OptimizeRequestDto;
  estimate: SearchEstimateDto | null;
  rows: SolvedBuildDto[];
  selected: SolvedBuildDto | null;
  compareTarget: SolvedBuildDto | null;
  selectedFingerprint: string | null;
  lockedStatMode: boolean;
  compareControls: CompareControls;
  horizon: number;
  notices: Notice[];
  isPathBusy: boolean;
  activePathJobId: string | null;
  pathProgress: PathProgressDto | null;
  pathSignature: string | null;
  paths: PathPreviewDto[];
  isAffinityBusy: boolean;
  activeAffinityJobId: string | null;
  affinityProgress: AffinityWatchProgressDto | null;
  affinitySignature: string | null;
  affinityPayload: AffinityWatchPayloadDto | null;
  error: string | null;
  isSearching: boolean;
  activeJobId: string | null;
  progress: SearchProgressDto | null;
  setWorkspace: (workspace: WorkspaceTab) => void;
  setCatalog: (catalog: CatalogDto) => void;
  patchRequest: (patch: Partial<OptimizeRequestDto>) => void;
  applyClass: (className: string) => void;
  setEstimate: (estimate: SearchEstimateDto | null) => void;
  setRows: (rows: SolvedBuildDto[]) => void;
  clearResults: (message?: string) => void;
  selectRow: (row: SolvedBuildDto | null) => void;
  setCompareTarget: (row: SolvedBuildDto | null) => void;
  patchCompareControls: (patch: Partial<CompareControls>) => void;
  setHorizon: (horizon: number) => void;
  setLockedStatMode: (lockedStatMode: boolean) => void;
  useRowAsLocks: (row: SolvedBuildDto) => void;
  setNotices: (notices: Notice[]) => void;
  pushNotice: (notice: Notice) => void;
  setError: (error: string | null) => void;
  setSearching: (isSearching: boolean) => void;
  setActiveJobId: (activeJobId: string | null) => void;
  setProgress: (progress: SearchProgressDto | null) => void;
  setPathBusy: (isPathBusy: boolean) => void;
  setActivePathJobId: (activePathJobId: string | null) => void;
  setPathProgress: (pathProgress: PathProgressDto | null) => void;
  setPaths: (paths: PathPreviewDto[], signature: string | null) => void;
  setAffinityBusy: (isAffinityBusy: boolean) => void;
  setActiveAffinityJobId: (activeAffinityJobId: string | null) => void;
  setAffinityProgress: (affinityProgress: AffinityWatchProgressDto | null) => void;
  setAffinityPayload: (affinityPayload: AffinityWatchPayloadDto | null, signature: string | null) => void;
}

export const defaultRequest: OptimizeRequestDto = {
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
  maxUpgrade: 25,
  fixedUpgrade: null,
  twoHanding: false,
  weaponName: null,
  affinity: null,
  aowName: null,
  weaponTypeKey: null,
  somberFilter: "all",
  objective: "max_ar",
  topK: 25,
};

export const objectiveOptions: ObjectiveId[] = [
  "max_ar",
  "max_ar_plus_bleed",
  "aow_first_hit",
  "aow_full_sequence",
];

export const useDesktopStore = create<DesktopState>((set) => ({
  activeWorkspace: "rankings",
  catalog: null,
  request: defaultRequest,
  estimate: null,
  rows: [],
  selected: null,
  compareTarget: null,
  selectedFingerprint: null,
  lockedStatMode: false,
  compareControls: {
    weaponTypeKey: null,
    weaponName: null,
    affinity: null,
    aowName: null,
    matchSelectedAow: true,
  },
  horizon: 40,
  notices: [],
  isPathBusy: false,
  activePathJobId: null,
  pathProgress: null,
  pathSignature: null,
  paths: [],
  isAffinityBusy: false,
  activeAffinityJobId: null,
  affinityProgress: null,
  affinitySignature: null,
  affinityPayload: null,
  error: null,
  isSearching: false,
  activeJobId: null,
  progress: null,
  setWorkspace: (activeWorkspace) => set({ activeWorkspace }),
  setCatalog: (catalog) => set({ catalog }),
  patchRequest: (patch) =>
    set((state) => ({
      request: { ...state.request, ...patch },
      rows: [],
      selected: null,
      compareTarget: null,
      selectedFingerprint: null,
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
        rows: [],
        selected: null,
        compareTarget: null,
        selectedFingerprint: null,
      };
    }),
  setEstimate: (estimate) => set({ estimate }),
  setRows: (rows) =>
    set((state) => ({
      rows,
      selected:
        rows.find((row) => rowFingerprint(row) === state.selectedFingerprint) ??
        rows[0] ??
        null,
      selectedFingerprint:
        rowFingerprint(
          rows.find((row) => rowFingerprint(row) === state.selectedFingerprint) ??
            rows[0] ??
            null,
        ),
    })),
  clearResults: (message) =>
    set((state) => ({
      rows: [],
      selected: null,
      compareTarget: null,
      selectedFingerprint: null,
      estimate: null,
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: message
        ? [...state.notices, { scope: "rankings", tone: "warning", message }]
        : state.notices,
    })),
  selectRow: (selected) =>
    set({
      selected,
      selectedFingerprint: rowFingerprint(selected),
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
    }),
  setCompareTarget: (compareTarget) =>
    set({
      compareTarget,
      paths: [],
      pathSignature: null,
    }),
  patchCompareControls: (patch) =>
    set((state) => ({
      compareControls: { ...state.compareControls, ...patch },
      compareTarget: null,
      paths: [],
      pathSignature: null,
    })),
  setHorizon: (horizon) =>
    set({
      horizon,
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
    }),
  setLockedStatMode: (lockedStatMode) => set({ lockedStatMode }),
  useRowAsLocks: (row) =>
    set((state) => ({
      request: {
        ...state.request,
        weaponName: row.weaponName,
        affinity: row.affinity,
        aowName: row.aowName,
        weaponTypeKey: null,
        somberFilter: "all",
        maxUpgrade: row.upgrade,
        fixedUpgrade: row.upgrade,
        lockStr: row.stats.strStat,
        lockDex: row.stats.dex,
        lockInt: row.stats.intStat,
        lockFai: row.stats.fai,
        lockArc: row.stats.arc,
      },
      lockedStatMode: true,
      rows: [],
      selected: null,
      compareTarget: null,
      selectedFingerprint: rowFingerprint(row),
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: [
        ...state.notices,
        { scope: "rankings", tone: "info", message: "Locked selected result; rerun search for exact locked stats." },
      ],
    })),
  setNotices: (notices) => set({ notices }),
  pushNotice: (notice) =>
    set((state) => ({
      notices: [...state.notices.filter((entry) => entry.scope !== notice.scope), notice],
    })),
  setError: (error) => set({ error }),
  setSearching: (isSearching) => set({ isSearching }),
  setActiveJobId: (activeJobId) => set({ activeJobId }),
  setProgress: (progress) => set({ progress }),
  setPathBusy: (isPathBusy) => set({ isPathBusy }),
  setActivePathJobId: (activePathJobId) => set({ activePathJobId }),
  setPathProgress: (pathProgress) => set({ pathProgress }),
  setPaths: (paths, pathSignature) => set({ paths, pathSignature }),
  setAffinityBusy: (isAffinityBusy) => set({ isAffinityBusy }),
  setActiveAffinityJobId: (activeAffinityJobId) => set({ activeAffinityJobId }),
  setAffinityProgress: (affinityProgress) => set({ affinityProgress }),
  setAffinityPayload: (affinityPayload, affinitySignature) =>
    set({ affinityPayload, affinitySignature }),
}));
