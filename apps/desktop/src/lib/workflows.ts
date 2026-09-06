import { api } from "./api";
import { buildOptimizeRequest, stableSignature } from "./session";
import { progressSignature, startAdaptivePolling } from "./polling";
import { useDesktopStore } from "./state";
import { OptimizeRequestDto, SearchProgressDto, SolvedBuildDto } from "./types";

let searchQueue: Promise<unknown> = Promise.resolve();

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

export function runSearchRequestForRows(
  request: OptimizeRequestDto,
  signal?: AbortSignal,
  onProgress?: (progress: SearchProgressDto | null) => void,
  onStarted?: (jobId: string) => void,
): Promise<SolvedBuildDto[]> {
  const search = searchQueue.then(async () => {
    if (signal?.aborted) throw new DOMException("Search stopped.", "AbortError");
    const { jobId } = await api.startSearch(request);
    onStarted?.(jobId);
    return await pollSearchRows(jobId, signal, onProgress);
  });
  // Cancellation only requests a stop; the next owner waits for terminal status.
  searchQueue = search.then(() => undefined, () => undefined);
  return search;
}

async function pollSearchRows(jobId: string, signal?: AbortSignal, onProgress?: (progress: SearchProgressDto | null) => void): Promise<SolvedBuildDto[]> {
  return await new Promise((resolve, reject) => {
    let settled = false;
    let cancelling = false;
    let stopPolling = () => {};
    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", abort);
      stopPolling();
      callback();
    };
    const abort = () => {
      if (settled || cancelling) return;
      cancelling = true;
      void api.cancelSearch(jobId).catch((error) => finish(() => reject(error)));
    };
    signal?.addEventListener("abort", abort, { once: true });
    const polling = startAdaptivePolling({
      poll: () => api.searchStatus(jobId),
      progressKey: (status) => progressSignature(status.progress),
      onStatus: (status) => {
        if (!cancelling) onProgress?.(status.progress);
        if (!status.finished) return false;
        if (cancelling) {
          finish(() => reject(new DOMException("Search stopped.", "AbortError")));
        } else if (status.finished.error) {
          finish(() => reject(new Error(status.finished!.error!)));
        } else if (status.finished.cancelled) {
          finish(() => reject(new DOMException("Search stopped.", "AbortError")));
        } else {
          finish(() => resolve(status.finished!.rows));
        }
        return true;
      },
      onMissing: () => finish(() => reject(new Error("Search job disappeared before returning a result."))),
      onError: (error) => finish(() => reject(error)),
    });
    stopPolling = polling.stop;
    if (settled) polling.stop();
    else if (signal?.aborted) abort();
  });
}
