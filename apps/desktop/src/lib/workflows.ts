import { api, hasTauriRuntime } from "./api";
import { buildOptimizeRequest } from "./session";
import { useDesktopStore } from "./state";
import { OptimizeRequestDto, SolvedBuildDto } from "./types";

export async function runSearchFromStore() {
  const state = useDesktopStore.getState();
  const request = buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
  state.setSearching(true);
  state.setError(null);
  state.setProgress(null);
  try {
    const estimate = await api.estimateSearchSpace(request);
    useDesktopStore.getState().setEstimate(estimate);
    if (estimate.combinations <= 0) {
      useDesktopStore.getState().clearResults("No valid search space for current constraints.");
      useDesktopStore.getState().setSearching(false);
      return;
    }
    if (hasTauriRuntime()) {
      const { jobId } = await api.startSearch(request);
      useDesktopStore.getState().setActiveJobId(jobId);
    } else {
      const rows = await api.runSearch(request);
      useDesktopStore.getState().setRows(rows);
      useDesktopStore.getState().setSearching(false);
    }
  } catch (error) {
    useDesktopStore.getState().setError(error instanceof Error ? error.message : String(error));
    useDesktopStore.getState().setSearching(false);
  }
}

export async function runSearchRequestForRows(request: OptimizeRequestDto): Promise<SolvedBuildDto[]> {
  const estimate = await api.estimateSearchSpace(request);
  if (estimate.combinations <= 0) {
    throw new Error("No valid search space for current constraints.");
  }

  if (!hasTauriRuntime()) {
    return await api.runSearch(request);
  }

  const { jobId } = await api.startSearch(request);
  return await pollSearchRows(jobId);
}

async function pollSearchRows(jobId: string): Promise<SolvedBuildDto[]> {
  return await new Promise((resolve, reject) => {
    let disposed = false;
    const interval = window.setInterval(async () => {
      if (disposed) {
        return;
      }
      try {
        const status = await api.searchStatus(jobId);
        if (!status) {
          disposed = true;
          window.clearInterval(interval);
          reject(new Error("Search job disappeared before returning a result."));
          return;
        }
        if (!status.finished) {
          return;
        }
        disposed = true;
        window.clearInterval(interval);
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
        disposed = true;
        window.clearInterval(interval);
        reject(error);
      }
    }, 200);
  });
}
