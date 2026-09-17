import { api } from "./api";
import { buildOptimizeRequest, stableSignature } from "./session";
import { progressSignature } from "./polling";
import { createNativeJobQueue } from "./native-jobs";
import { useDesktopStore } from "./state";
import { OptimizeRequestDto, SearchJobStatusDto, SearchProgressDto, SolvedBuildDto } from "./types";

const searchQueue = createNativeJobQueue<SearchJobStatusDto>(
  jobId => api.searchStatus(jobId), jobId => api.cancelSearch(jobId),
  status => progressSignature(status.progress),
);

export async function runSearchFromStore(
  requestOverride?: OptimizeRequestDto,
  cancellationRequested: () => boolean = () => false,
): Promise<boolean> {
  const state = useDesktopStore.getState();
  if (state.isExporting) return false;
  const request = requestOverride ?? buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  const signature = stableSignature(request);
  const generation = state.beginSearch(signature);
  const controller = new AbortController();
  const isCurrent = () => {
    const current = useDesktopStore.getState();
    return current.searchGeneration === generation && current.activeSearchSignature === signature;
  };
  const unsubscribe = useDesktopStore.subscribe(() => {
    if (!isCurrent()) controller.abort();
  });
  state.setError(null);
  try {
    const rows = await runSearchRequestForRows(request, controller.signal, (progress) => {
      if (isCurrent()) useDesktopStore.getState().setProgress(progress);
    }, (jobId) => {
      if (isCurrent()) useDesktopStore.getState().setActiveJobId(jobId);
      if (cancellationRequested()) controller.abort();
    });
    if (!isCurrent()) return false;
    useDesktopStore.getState().setRows(rows);
    return true;
  } catch (error) {
    if (isCurrent()) {
      if (error instanceof DOMException && error.name === "AbortError") {
        useDesktopStore.getState().pushNotice({
          scope: "rankings", tone: "warning", message: "Search stopped. Previous results were retained.",
        });
      } else {
        useDesktopStore.getState().setError(error instanceof Error ? error.message : String(error));
      }
    }
    return false;
  } finally {
    unsubscribe();
    if (isCurrent()) {
      const current = useDesktopStore.getState();
      current.setSearching(false);
      current.setActiveJobId(null);
      current.setProgress(null);
    }
  }
}

export async function runSearchRequestForRows(
  request: OptimizeRequestDto,
  signal?: AbortSignal,
  onProgress?: (progress: SearchProgressDto | null) => void,
  onStarted?: (jobId: string) => void,
): Promise<SolvedBuildDto[]> {
  const finished = await searchQueue(() => api.startSearch(request), signal,
    status => onProgress?.(status.progress), onStarted);
  return finished.rows;
}
