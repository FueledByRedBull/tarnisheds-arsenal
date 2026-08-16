import { api } from "./api";
import { buildOptimizeRequest, stableSignature } from "./session";
import { INITIAL_POLL_DELAY_MS, nextPollDelay, progressSignature } from "./polling";
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
