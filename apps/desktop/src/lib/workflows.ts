import { api } from "./api";
import { buildOptimizeRequest, stableSignature } from "./session";
import { progressSignature } from "./polling";
import { createNativeJobQueue, isCancelled } from "./native-jobs";
import { createRunSlot } from "./run-slot";
import { useDesktopStore } from "./state";
import { OptimizeRequestDto, SearchJobStatusDto, SearchProgressDto, SolvedBuildDto } from "./types";
import { errorMessage } from "./format";

const searchQueue = createNativeJobQueue<SearchJobStatusDto>(
  jobId => api.searchStatus(jobId), jobId => api.cancelSearch(jobId),
  status => progressSignature(status.progress),
);

// The Rankings search, whichever control started it. Rankings, comparisons and CSV export share
// its native queue; only Rankings publishes into the store's search state.
const searchSlot = createRunSlot<SearchJobStatusDto>({
  queue: searchQueue,
  read: (state) => ({ busy: state.isSearching, generation: state.searchGeneration, signature: state.activeSearchSignature, jobId: state.activeJobId }),
  begin: (signature) => useDesktopStore.getState().beginSearch(signature),
  setJobId: (jobId) => useDesktopStore.getState().setActiveJobId(jobId),
  end: () => useDesktopStore.setState({ isSearching: false, activeJobId: null, progress: null }),
});

/** Stops the Rankings search, whether it is still queued or already running natively. */
export function cancelRankingsSearch(): void {
  searchSlot.cancel();
}

// A search asked for during a CSV export waits for it. At most one waits, and the latest request
// replaces it. It belongs to the profile and catalog it was asked on: a profile switch drops it.
let stopWaitingForExport: (() => void) | null = null;

/** Locks the query to a result's equipment, upgrade and stats, then searches again. */
export function lockRowAndSearch(row: SolvedBuildDto): Promise<boolean> {
  useDesktopStore.getState().useRowAsLocks(row);
  return runSearchFromStore();
}

export async function runSearchFromStore(requestOverride?: OptimizeRequestDto): Promise<boolean> {
  const state = useDesktopStore.getState();
  if (state.isExporting) {
    stopWaitingForExport?.();
    const { catalog, request: { profileId } } = state;
    const unsubscribe = useDesktopStore.subscribe((current) => {
      if (!current.searchWaitingForExport) {
        unsubscribe();
        stopWaitingForExport = null;
        return;
      }
      if (current.isExporting) return;
      unsubscribe();
      stopWaitingForExport = null;
      current.setSearchWaitingForExport(false);
      if (current.catalogStatus === "ready" && current.catalog === catalog && current.request.profileId === profileId) {
        // Start outside this store notification so the search's own updates are not nested in it.
        queueMicrotask(() => void runSearchFromStore(requestOverride));
      }
    });
    stopWaitingForExport = unsubscribe;
    state.setSearchWaitingForExport(true);
    return false;
  }
  const request = requestOverride ?? buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  const signature = stableSignature(request);
  const token = searchSlot.begin(signature);
  state.setError(null);
  try {
    const { rows, setups, jobId } = await searchSlot.run(token, () => api.startSearch(request), (status) => {
      useDesktopStore.getState().setProgress(status.progress);
    });
    if (!searchSlot.accepts(useDesktopStore.getState(), token, jobId)) return false;
    useDesktopStore.getState().setRows(rows, setups);
    return true;
  } catch (error) {
    if (searchSlot.owns(useDesktopStore.getState(), token)) {
      useDesktopStore.getState().recheckResultsStale(signature);
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
    searchSlot.end(token);
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
