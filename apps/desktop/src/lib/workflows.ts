import { api } from "./api";
import { buildOptimizeRequest, stableSignature } from "./session";
import { progressSignature } from "./polling";
import { createNativeJobQueue, isCancelled } from "./native-jobs";
import { useDesktopStore } from "./state";
import { OptimizeRequestDto, SearchJobStatusDto, SearchProgressDto, SolvedBuildDto } from "./types";
import { errorMessage } from "./format";

const searchQueue = createNativeJobQueue<SearchJobStatusDto>(
  jobId => api.searchStatus(jobId), jobId => api.cancelSearch(jobId),
  status => progressSignature(status.progress),
);

// The Rankings search that owns the store's search generation, whichever control started it.
let activeSearch: { generation: number; controller: AbortController } | null = null;

// Cancel reaches a search still waiting in the queue, before it has a native job to cancel.
export function cancelSearchFromStore(generation: number): void {
  if (activeSearch?.generation === generation) activeSearch.controller.abort();
}

export async function runSearchFromStore(requestOverride?: OptimizeRequestDto): Promise<boolean> {
  const state = useDesktopStore.getState();
  if (state.isExporting) return false;
  const request = requestOverride ?? buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  const signature = stableSignature(request);
  const generation = state.beginSearch(signature);
  const controller = new AbortController();
  activeSearch = { generation, controller };
  const isCurrent = () => {
    const current = useDesktopStore.getState();
    return current.searchGeneration === generation && current.activeSearchSignature === signature;
  };
  const unsubscribe = useDesktopStore.subscribe(() => {
    if (!isCurrent()) controller.abort();
  });
  state.setError(null);
  try {
    const { rows, setups } = await searchQueue(() => api.startSearch(request), controller.signal, (status) => {
      if (isCurrent()) useDesktopStore.getState().setProgress(status.progress);
    }, (jobId) => {
      if (isCurrent()) useDesktopStore.getState().setActiveJobId(jobId);
    });
    if (!isCurrent()) return false;
    useDesktopStore.getState().setRows(rows, setups);
    return true;
  } catch (error) {
    if (isCurrent()) {
      if (isCancelled(error)) {
        useDesktopStore.getState().pushNotice({
          scope: "rankings", tone: "warning", message: "Search stopped. Previous results were retained.",
        });
      } else {
        useDesktopStore.getState().setError(errorMessage(error));
      }
    }
    return false;
  } finally {
    unsubscribe();
    if (activeSearch?.controller === controller) activeSearch = null;
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
