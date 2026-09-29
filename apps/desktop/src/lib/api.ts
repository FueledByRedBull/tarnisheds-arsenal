import { invoke } from "@tauri-apps/api/core";
import { createNativeJobQueue } from "./native-jobs";
import { errorMessage } from "./format";
import type {
  AnalysisJobStatusDto,
  ArBleedFrontierRequestDto,
  ArBleedFrontierPointDto,
  AffinityWatchRequestDto,
  AffinityWatchJobStatusDto,
  CatalogDto,
  CompatibleAowsForAffinityRequestDto,
  DataManifestDto,
  OptimizeRequestDto,
  PathJobStatusDto,
  PathPreviewRequestDto,
  SearchJobStatusDto,
  StartSearchResponseDto,
  SolvedBuildDto,
  SolveBuildRequestDto,
  StartPathPreviewRequestDto,
  UpgradeSeriesRequestDto,
  UpgradePointDto,
  WeaponProfileDto,
  WeaponProfileRequestDto,
} from "./types";

const analysisQueue = createNativeJobQueue<AnalysisJobStatusDto>(
  jobId => call("get_analysis_status", { jobId }),
  jobId => call("cancel_analysis", { jobId }),
);

export const hasTauriRuntime = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (hasTauriRuntime()) {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      throw new Error(errorMessage(error));
    }
  }
  if (import.meta.env.DEV) {
    const { mockInvoke } = await import("./browser-backend");
    return mockInvoke<T>(command, args);
  }
  throw new Error("Tauri runtime is required outside the explicit dev browser preview.");
}

export const api = {
  profiles: () => call<DataManifestDto[]>("get_profiles"),
  catalog: (profileId: string) => call<CatalogDto>("get_catalog", { profileId }),
  weaponProfile: (profileId: string, weaponName: string, affinity: string | null) =>
    call<WeaponProfileDto>("get_weapon_profile", {
      request: { profileId, weaponName, affinity } satisfies WeaponProfileRequestDto,
    }),
  startSearch: (request: OptimizeRequestDto) =>
    call<StartSearchResponseDto>("start_search", { request }),
  cancelSearch: (jobId: string) =>
    call<boolean>("cancel_search", { jobId }),
  searchStatus: (jobId: string) =>
    call<SearchJobStatusDto | null>("get_search_status", { jobId }),
  solveBuild: async (
    base: OptimizeRequestDto,
    weaponName: string,
    affinity: string | null,
    aowName: string | null,
    signal?: AbortSignal,
  ): Promise<SolvedBuildDto | null> => {
    if (signal?.aborted) throw new DOMException("Calculation stopped.", "AbortError");
    const request = { base, weaponName, affinity, aowName } satisfies SolveBuildRequestDto;
    if (!hasTauriRuntime()) return call("solve_build", { request });
    const finished = await analysisQueue(() => call("start_solve_build", { request }), signal);
    if (finished.kind !== "solve_build") throw new Error("Unexpected native calculation result.");
    return finished.result;
  },
  arBleedFrontier: async (
    base: OptimizeRequestDto,
    solved: SolvedBuildDto,
    signal?: AbortSignal,
  ): Promise<ArBleedFrontierPointDto[]> => {
    if (signal?.aborted) throw new DOMException("Calculation stopped.", "AbortError");
    const request = { base, solved } satisfies ArBleedFrontierRequestDto;
    if (!hasTauriRuntime()) return call("ar_bleed_frontier", { request });
    const finished = await analysisQueue(() => call("start_ar_bleed_frontier", { request }), signal);
    if (finished.kind !== "ar_bleed_frontier") throw new Error("Unexpected native calculation result.");
    return finished.frontier;
  },
  buildUpgradeSeries: async (
    base: OptimizeRequestDto,
    solved: SolvedBuildDto,
    maxUpgrade: number,
    signal?: AbortSignal,
  ): Promise<UpgradePointDto[]> => {
    if (signal?.aborted) throw new DOMException("Calculation stopped.", "AbortError");
    const request = { base, solved, maxUpgrade } satisfies UpgradeSeriesRequestDto;
    if (!hasTauriRuntime()) return call("build_upgrade_series", { request });
    const finished = await analysisQueue(() => call("start_upgrade_series", { request }), signal);
    if (finished.kind !== "upgrade_series") throw new Error("Unexpected native calculation result.");
    return finished.points;
  },
  affinitiesForWeapon: (profileId: string, weaponName: string) =>
    call<string[]>("affinities_for_weapon", { profileId, weaponName }),
  compatibleAowNamesForAffinity: (profileId: string, affinity: string | null) =>
    call<string[]>("compatible_aow_names_for_affinity", {
      request: { profileId, affinity } satisfies CompatibleAowsForAffinityRequestDto,
    }),
  startPathPreview: (requests: PathPreviewRequestDto[]) =>
    call<StartSearchResponseDto>("start_path_preview", {
      request: { requests } satisfies StartPathPreviewRequestDto,
    }),
  cancelPathPreview: (jobId: string) =>
    call<boolean>("cancel_path_preview", { jobId }),
  pathPreviewStatus: (jobId: string) =>
    call<PathJobStatusDto | null>("get_path_preview_status", { jobId }),
  startAffinityWatch: (
    base: OptimizeRequestDto,
    solved: SolvedBuildDto,
    levelsAhead: number,
  ) =>
    call<StartSearchResponseDto>("start_affinity_watch", {
      request: { base, solved, levelsAhead } satisfies AffinityWatchRequestDto,
    }),
  cancelAffinityWatch: (jobId: string) =>
    call<boolean>("cancel_affinity_watch", { jobId }),
  affinityWatchStatus: (jobId: string) =>
    call<AffinityWatchJobStatusDto | null>("get_affinity_watch_status", { jobId }),
};
