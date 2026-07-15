import { api, hasTauriRuntime } from "./api";
import { buildOptimizeRequest, stableSignature } from "./session";
import { INITIAL_POLL_DELAY_MS, nextPollDelay, progressSignature } from "./polling";
import { useDesktopStore } from "./state";
import { OptimizeRequestDto, SolvedBuildDto } from "./types";

export async function runSearchFromStore() {
  const state = useDesktopStore.getState();
  const request = buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  const signature = stableSignature(request);
  const generation = state.beginSearch(signature);
  state.setError(null);
  try {
    if (hasTauriRuntime()) {
      const { jobId } = await api.startSearch(request);
      const current = useDesktopStore.getState();
      if (
        current.searchGeneration !== generation ||
        current.activeSearchSignature !== signature
      ) {
        await api.cancelSearch(jobId);
        return;
      }
      current.setActiveJobId(jobId);
    } else {
      const estimate = await api.estimateSearchSpace(request);
      let current = useDesktopStore.getState();
      if (
        current.searchGeneration !== generation ||
        current.activeSearchSignature !== signature
      ) return;
      current.setEstimate(estimate);
      if (estimate.combinations <= 0) {
        current.clearResults("No valid search space for current constraints.");
        current.setSearching(false);
        return;
      }
      const rows = await api.runSearch(request);
      current = useDesktopStore.getState();
      if (
        current.searchGeneration !== generation ||
        current.activeSearchSignature !== signature
      ) return;
      current.setRows(rows);
      current.setSearching(false);
    }
  } catch (error) {
    const current = useDesktopStore.getState();
    if (
      current.searchGeneration === generation &&
      current.activeSearchSignature === signature
    ) {
      current.setError(error instanceof Error ? error.message : String(error));
      current.setSearching(false);
    }
  }
}

export async function runSearchRequestForRows(request: OptimizeRequestDto): Promise<SolvedBuildDto[]> {
  if (!hasTauriRuntime()) {
    const estimate = await api.estimateSearchSpace(request);
    if (estimate.combinations <= 0) {
      throw new Error("No valid search space for current constraints.");
    }
    return await api.runSearch(request);
  }

  const { jobId } = await api.startSearch(request);
  return await pollSearchRows(jobId);
}

async function pollSearchRows(jobId: string): Promise<SolvedBuildDto[]> {
  return await new Promise((resolve, reject) => {
    let finished = false;
    let timer: number | undefined;
    let delay = INITIAL_POLL_DELAY_MS;
    let lastProgress = "";

    async function poll() {
      if (finished) return;
      try {
        const status = await api.searchStatus(jobId);
        if (!status) {
          finished = true;
          reject(new Error("Search job disappeared before returning a result."));
          return;
        }
        if (!status.finished) {
          const nextProgress = progressSignature(status.progress);
          delay = nextPollDelay(delay, nextProgress !== lastProgress);
          lastProgress = nextProgress;
          timer = window.setTimeout(() => void poll(), delay);
          return;
        }
        finished = true;
        if (status.finished.error) {
          reject(new Error(status.finished.error));
          return;
        }
        if (status.finished.cancelled) {
          reject(new Error("Search stopped."));
          return;
        }
        resolve(status.finished.rows);
      } catch (error) {
        finished = true;
        if (timer !== undefined) window.clearTimeout(timer);
        reject(error);
      }
    }

    void poll();
  });
}
