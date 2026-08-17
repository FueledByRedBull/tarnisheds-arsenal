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

export async function runSearchRequestForRows(request: OptimizeRequestDto): Promise<SolvedBuildDto[]> {
  const { jobId } = await api.startSearch(request);
  return await pollSearchRows(jobId);
}

async function pollSearchRows(jobId: string): Promise<SolvedBuildDto[]> {
  return await new Promise((resolve, reject) => {
    startAdaptivePolling({
      poll: () => api.searchStatus(jobId),
      progressKey: (status) => progressSignature(status.progress),
      onStatus: (status) => {
        if (!status.finished) return false;
        if (status.finished.error) {
          reject(new Error(status.finished.error));
        } else if (status.finished.cancelled) {
          reject(new Error("Search stopped."));
        } else {
          resolve(status.finished.rows);
        }
        return true;
      },
      onMissing: () => reject(new Error("Search job disappeared before returning a result.")),
      onError: reject,
    });
  });
}
