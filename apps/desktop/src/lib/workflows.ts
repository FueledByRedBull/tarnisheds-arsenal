import { api, hasTauriRuntime } from "./api";
import { buildOptimizeRequest } from "./session";
import { useDesktopStore } from "./state";

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
