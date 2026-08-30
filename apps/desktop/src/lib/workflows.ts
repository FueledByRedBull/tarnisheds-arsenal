import { api } from "./api";
import { buildOptimizeRequest, stableSignature } from "./session";
import { progressSignature, startAdaptivePolling } from "./polling";
import { useDesktopStore } from "./state";
import { OptimizeRequestDto, SolvedBuildDto } from "./types";

export async function runSearchFromStore(
  requestOverride?: OptimizeRequestDto,
  cancellationRequested: () => boolean = () => false,
): Promise<boolean> {
  const state = useDesktopStore.getState();
  const request = requestOverride ?? buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  const signature = stableSignature(request);
  const generation = state.beginSearch(signature);
  state.setError(null);
  try {
    const { jobId } = await api.startSearch(request);
    const current = useDesktopStore.getState();
    if (
      current.searchGeneration !== generation ||
      current.activeSearchSignature !== signature
    ) {
      await api.cancelSearch(jobId);
      return false;
    }
    current.setActiveJobId(jobId);
    if (cancellationRequested()) await api.cancelSearch(jobId);
    return true;
  } catch (error) {
    const current = useDesktopStore.getState();
    if (
      current.searchGeneration === generation &&
      current.activeSearchSignature === signature
    ) {
      current.setError(error instanceof Error ? error.message : String(error));
      current.setSearching(false);
    }
    return false;
  }
}

export async function runSearchRequestForRows(request: OptimizeRequestDto, signal?: AbortSignal): Promise<SolvedBuildDto[]> {
  if (signal?.aborted) throw new Error("cancelled");
  const { jobId } = await api.startSearch(request);
  if (signal?.aborted) {
    await api.cancelSearch(jobId);
    throw new Error("cancelled");
  }
  return await pollSearchRows(jobId, signal);
}

async function pollSearchRows(jobId: string, signal?: AbortSignal): Promise<SolvedBuildDto[]> {
  return await new Promise((resolve, reject) => {
    let settled = false;
    let stopPolling = () => {};
    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", abort);
      stopPolling();
      callback();
    };
    const abort = () => {
      if (settled) return;
      void api.cancelSearch(jobId);
      finish(() => reject(new Error("cancelled")));
    };
    signal?.addEventListener("abort", abort, { once: true });
    const polling = startAdaptivePolling({
      poll: () => api.searchStatus(jobId),
      progressKey: (status) => progressSignature(status.progress),
      onStatus: (status) => {
        if (!status.finished) return false;
        if (status.finished.error) {
          finish(() => reject(new Error(status.finished!.error!)));
        } else if (status.finished.cancelled) {
          finish(() => reject(new Error("Search stopped.")));
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
