import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { PATH_RUN, runAnalysis } from "./hooks";
import { useDesktopStore } from "./state";
import type { PathJobStatusDto } from "./types";

const running = (jobId: string): PathJobStatusDto => ({ progress: { jobId, checked: 0, total: 1, title: "selected", level: 0 }, finished: null });
const finished = (jobId: string): PathJobStatusDto => ({ progress: null, finished: { jobId, cancelled: false, paths: [], error: null } });

beforeEach(() => {
  vi.useFakeTimers();
  useDesktopStore.setState({ isPathBusy: false, activePathSignature: null, activePathJobId: null, pathProgress: null,
    resultsStale: false, notices: [], error: null });
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("an analysis run", () => {
  it("is refused on out-of-date results before it takes the slot, and says why", async () => {
    useDesktopStore.setState({ resultsStale: true });
    const launch = vi.fn(async () => ({ jobId: "a" }));
    const report = vi.fn();

    await runAnalysis(PATH_RUN, "inputs", launch, vi.fn(), report);

    expect(launch).not.toHaveBeenCalled();
    expect(useDesktopStore.getState()).toMatchObject({ isPathBusy: false, activePathSignature: null });
    expect(useDesktopStore.getState().notices).toEqual([{ scope: "paths", tone: "warning",
      message: "The selected build belongs to the previous query. Go to Rankings, then press Run updated search." }]);
    expect(report).not.toHaveBeenCalledWith("cancelled");
  });

  it("ends its own busy state when its job is no longer the slot's current job", async () => {
    let done = false;
    vi.spyOn(api, "pathPreviewStatus").mockImplementation(async (jobId) => (done ? finished(jobId) : running(jobId)));
    const publish = vi.fn();
    const result = runAnalysis(PATH_RUN, "inputs", async () => ({ jobId: "a" }), publish, vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    expect(useDesktopStore.getState()).toMatchObject({ isPathBusy: true, activePathJobId: "a" });

    useDesktopStore.getState().setActivePathJobId("other");
    done = true;
    await vi.advanceTimersByTimeAsync(200);
    await result;

    expect(publish).not.toHaveBeenCalled();
    expect(useDesktopStore.getState()).toMatchObject({ isPathBusy: false, activePathJobId: null, pathProgress: null });
  });
});
