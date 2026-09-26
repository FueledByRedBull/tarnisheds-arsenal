import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "./api";
import { defaultRequest, useDesktopStore } from "./state";
import { runSearchFromStore, runSearchRequestForRows } from "./workflows";
import type { SearchJobStatusDto, SolvedBuildDto } from "./types";

vi.mock("./api", () => ({ api: { startSearch: vi.fn(), searchStatus: vi.fn(), cancelSearch: vi.fn() } }));

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  useDesktopStore.setState({
    request: defaultRequest, catalog: null, rows: [], notices: [], progress: null,
    isExporting: false, isSearching: false, activeJobId: null, activeSearchSignature: null, error: null,
  });
});
afterEach(() => vi.useRealTimers());

it.each(["comparison", "rankings"])("waits for cancelled native work before starting replacement %s", async (owner) => {
  let finished = false;
  vi.mocked(api.startSearch).mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "new" });
  vi.mocked(api.cancelSearch).mockResolvedValue(true);
  vi.mocked(api.searchStatus).mockImplementation(async (jobId) => ({
    progress: null,
    finished: jobId === "new" || finished ? { jobId, rows: [], cancelled: jobId === "old", error: null } : null,
  }));
  const controller = new AbortController();
  const old = runSearchRequestForRows(defaultRequest, controller.signal).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  const replacement = owner === "rankings"
    ? runSearchFromStore(defaultRequest)
    : runSearchRequestForRows(defaultRequest);
  await vi.advanceTimersByTimeAsync(0);
  expect(api.cancelSearch).toHaveBeenCalledWith("old");
  expect(api.startSearch).toHaveBeenCalledTimes(1);
  finished = true;
  await vi.advanceTimersByTimeAsync(200);
  expect(await old).toBeInstanceOf(Error);
  await replacement;
  expect(api.startSearch).toHaveBeenCalledTimes(2);
  expect(useDesktopStore.getState().error).toBeNull();
});

it("keeps a replacement queued when cancellation IPC fails until the worker finishes", async () => {
  let oldFinished = false;
  vi.mocked(api.startSearch).mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "new" });
  vi.mocked(api.cancelSearch).mockRejectedValueOnce(new Error("cancel IPC failed"));
  vi.mocked(api.searchStatus).mockImplementation(async (jobId) => ({
    progress: null,
    finished: jobId === "new" || (jobId === "old" && oldFinished)
      ? { jobId, rows: [], cancelled: false, error: null }
      : null,
  }));
  const controller = new AbortController();
  const old = runSearchRequestForRows(defaultRequest, controller.signal).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  const replacement = runSearchRequestForRows(defaultRequest);
  await vi.advanceTimersByTimeAsync(0);

  expect(api.cancelSearch).toHaveBeenCalledWith("old");
  expect(api.startSearch).toHaveBeenCalledTimes(1);
  expect(await old).toMatchObject({ message: "cancel IPC failed" });

  oldFinished = true;
  await vi.advanceTimersByTimeAsync(200);
  await expect(replacement).resolves.toEqual([]);
  expect(api.startSearch).toHaveBeenCalledTimes(2);
});

it("keeps a replacement queued across a status failure and recovers the running worker", async () => {
  let oldFinished = false;
  let failedInitialStatus = false;
  vi.mocked(api.startSearch).mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "new" });
  vi.mocked(api.cancelSearch).mockResolvedValue(true);
  vi.mocked(api.searchStatus).mockImplementation(async (jobId) => {
    if (jobId === "old" && !failedInitialStatus) {
      failedInitialStatus = true;
      throw new Error("status IPC failed");
    }
    return {
      progress: null,
      finished: jobId === "new" || (jobId === "old" && oldFinished)
        ? { jobId, rows: [], cancelled: jobId === "old", error: null }
        : null,
    };
  });
  const old = runSearchRequestForRows(defaultRequest).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  const replacement = runSearchRequestForRows(defaultRequest);
  await vi.advanceTimersByTimeAsync(0);

  expect(await old).toMatchObject({ message: "status IPC failed" });
  expect(api.cancelSearch).toHaveBeenCalledWith("old");
  expect(api.startSearch).toHaveBeenCalledTimes(1);

  oldFinished = true;
  await vi.advanceTimersByTimeAsync(200);
  await expect(replacement).resolves.toEqual([]);
  expect(api.startSearch).toHaveBeenCalledTimes(2);
});

it("bounds unknown-worker reconciliation and retries after a known missing job", async () => {
  let statusMode: "failed" | "missing" = "failed";
  vi.mocked(api.startSearch).mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "latest" });
  vi.mocked(api.cancelSearch).mockResolvedValue(true);
  vi.mocked(api.searchStatus).mockImplementation(async (jobId) => {
    if (jobId === "latest") {
      return { progress: null, finished: { jobId, rows: [], cancelled: false, error: null } };
    }
    if (statusMode === "failed") throw new Error("status IPC failed");
    return null;
  });

  const old = runSearchRequestForRows(defaultRequest).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  const blocked = runSearchRequestForRows(defaultRequest).catch(error => error);
  await vi.advanceTimersByTimeAsync(7_000);

  expect(await old).toMatchObject({ message: "status IPC failed" });
  expect((await blocked).message).toContain("Worker state is unknown");
  expect(api.startSearch).toHaveBeenCalledTimes(1);

  statusMode = "missing";
  await expect(runSearchRequestForRows(defaultRequest)).resolves.toEqual([]);
  expect(api.startSearch).toHaveBeenCalledTimes(2);
});

it("bounds reconciliation when cancellation fails and the worker stays running", async () => {
  let statusMode: "running" | "missing" = "running";
  vi.mocked(api.startSearch).mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "latest" });
  vi.mocked(api.cancelSearch).mockRejectedValue(new Error("cancel IPC failed"));
  vi.mocked(api.searchStatus).mockImplementation(async (jobId) => {
    if (jobId === "latest") {
      return { progress: null, finished: { jobId, rows: [], cancelled: false, error: null } };
    }
    return statusMode === "running" ? { progress: null, finished: null } : null;
  });

  const controller = new AbortController();
  const old = runSearchRequestForRows(defaultRequest, controller.signal).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  await vi.advanceTimersByTimeAsync(0);
  const blocked = runSearchRequestForRows(defaultRequest).catch(error => error);
  await vi.advanceTimersByTimeAsync(7_000);

  expect(await old).toMatchObject({ message: "cancel IPC failed" });
  expect((await blocked).message).toContain("Worker state is unknown");
  expect(api.startSearch).toHaveBeenCalledTimes(1);

  statusMode = "missing";
  await expect(runSearchRequestForRows(defaultRequest)).resolves.toEqual([]);
  expect(api.startSearch).toHaveBeenCalledTimes(2);
});

it("consumes a terminal status that races with cancellation IPC failure", async () => {
  let resolveOldStatus!: (status: {
    progress: null;
    finished: { jobId: string; rows: []; cancelled: boolean; error: null };
  }) => void;
  let firstOldStatus = true;
  vi.mocked(api.startSearch).mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "new" });
  vi.mocked(api.cancelSearch).mockRejectedValueOnce(new Error("cancel IPC failed"));
  vi.mocked(api.searchStatus).mockImplementation(async (jobId) => {
    if (jobId === "old" && firstOldStatus) {
      firstOldStatus = false;
      return await new Promise(resolve => { resolveOldStatus = resolve; });
    }
    return {
      progress: null,
      finished: { jobId, rows: [], cancelled: jobId === "old", error: null },
    };
  });

  const controller = new AbortController();
  const old = runSearchRequestForRows(defaultRequest, controller.signal).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  const replacement = runSearchRequestForRows(defaultRequest);
  await vi.advanceTimersByTimeAsync(0);
  expect(api.cancelSearch).toHaveBeenCalledWith("old");

  resolveOldStatus({ progress: null, finished: { jobId: "old", rows: [], cancelled: false, error: null } });
  await vi.advanceTimersByTimeAsync(0);
  expect(await old).toMatchObject({ message: "cancel IPC failed" });
  await expect(replacement).resolves.toEqual([]);
  expect(api.startSearch).toHaveBeenCalledTimes(2);
});

it("drops obsolete queued work and cancels a job whose start reply arrives late", async () => {
  let reply!: (value: { jobId: string }) => void;
  vi.mocked(api.startSearch).mockImplementationOnce(() => new Promise(resolve => { reply = resolve; }))
    .mockResolvedValue({ jobId: "latest" });
  vi.mocked(api.cancelSearch).mockResolvedValue(true);
  vi.mocked(api.searchStatus).mockImplementation(async (jobId) => ({
    progress: null, finished: { jobId, rows: [], cancelled: false, error: null },
  }));
  const first = new AbortController();
  const obsolete = new AbortController();
  const early = runSearchRequestForRows(defaultRequest, first.signal).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  first.abort();
  const skipped = runSearchRequestForRows(defaultRequest, obsolete.signal).catch(error => error);
  obsolete.abort();
  const latest = runSearchRequestForRows(defaultRequest);
  reply({ jobId: "late" });
  await vi.advanceTimersByTimeAsync(0);
  expect(await early).toBeInstanceOf(DOMException);
  expect(await skipped).toBeInstanceOf(DOMException);
  expect(await latest).toEqual([]);
  expect(api.cancelSearch).toHaveBeenCalledWith("late");
  expect(api.startSearch).toHaveBeenCalledTimes(2);
});

it("invalidates queued Rankings before it reaches the native backend", async () => {
  vi.mocked(api.startSearch).mockResolvedValue({ jobId: "comparison" });
  vi.mocked(api.searchStatus).mockResolvedValue({ progress: null, finished: null });
  const first = runSearchRequestForRows(defaultRequest);
  await vi.advanceTimersByTimeAsync(0);
  const rankings = runSearchFromStore(defaultRequest);
  useDesktopStore.getState().patchRequest({ twoHanding: !defaultRequest.twoHanding });
  vi.mocked(api.searchStatus).mockResolvedValue({
    progress: null, finished: { jobId: "comparison", rows: [], cancelled: false, error: null },
  });
  await vi.advanceTimersByTimeAsync(200);
  await first;
  expect(await rankings).toBe(false);
  expect(api.startSearch).toHaveBeenCalledTimes(1);
  expect(useDesktopStore.getState().error).toBeNull();
});

it("allows another search after a rejected start", async () => {
  vi.mocked(api.startSearch).mockRejectedValueOnce(new Error("Invalid request"))
    .mockResolvedValue({ jobId: "valid" });
  vi.mocked(api.searchStatus).mockResolvedValue({
    progress: null, finished: { jobId: "valid", rows: [], cancelled: false, error: null },
  });
  await expect(runSearchRequestForRows(defaultRequest)).rejects.toThrow("Invalid request");
  await expect(runSearchRequestForRows(defaultRequest)).resolves.toEqual([]);
});

it("does not publish a running Rankings result after an uncommitted numeric edit", async () => {
  let finish!: (status: SearchJobStatusDto) => void;
  vi.mocked(api.startSearch).mockResolvedValue({ jobId: "draft-edit" });
  vi.mocked(api.cancelSearch).mockResolvedValue(true);
  vi.mocked(api.searchStatus).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const running = runSearchFromStore(defaultRequest);
  await vi.advanceTimersByTimeAsync(0);
  useDesktopStore.getState().markResultsStale();
  finish({ progress: null, finished: { jobId: "draft-edit", rows: [], cancelled: false, error: null } });
  await vi.advanceTimersByTimeAsync(0);
  expect(await running).toBe(false);
  expect(api.cancelSearch).toHaveBeenCalledWith("draft-edit");
});

for (const invalidation of ["profile switch", "request edit"] as const) {
  it.each(["success", "cancelled", "error", "progress"] as const)(
    `ignores late %s after ${invalidation} without clearing replacement state`, async (outcome) => {
      const oldRows: SolvedBuildDto[] = [];
      const newRows: SolvedBuildDto[] = [];
      let completeOld!: (status: SearchJobStatusDto) => void;
      let completeNew!: (status: SearchJobStatusDto) => void;
      const oldStatus = new Promise<SearchJobStatusDto>(resolve => { completeOld = resolve; });
      const newStatus = new Promise<SearchJobStatusDto>(resolve => { completeNew = resolve; });
      vi.mocked(api.startSearch).mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "new" });
      vi.mocked(api.cancelSearch).mockResolvedValue(true);
      vi.mocked(api.searchStatus).mockImplementationOnce(() => oldStatus).mockImplementation(async (jobId) =>
        jobId === "new" ? newStatus : {
          progress: null, finished: { jobId, rows: oldRows, cancelled: false, error: null },
        });

      const old = runSearchFromStore(defaultRequest);
      await vi.advanceTimersByTimeAsync(0);
      if (invalidation === "profile switch") useDesktopStore.getState().beginProfileSwitch("convergence");
      else useDesktopStore.getState().patchRequest({ twoHanding: !defaultRequest.twoHanding });
      const retainedRows = useDesktopStore.getState().rows;
      const replacement = runSearchFromStore(useDesktopStore.getState().request);
      await vi.advanceTimersByTimeAsync(0);
      expect(api.startSearch).toHaveBeenCalledTimes(1);
      expect(api.cancelSearch).toHaveBeenCalledExactlyOnceWith("old");

      completeOld(outcome === "progress" ? {
        progress: { jobId: "old", checked: 10, total: 10, eligible: 1, bestScore: 999, elapsedMs: 100 },
        finished: null,
      } : {
        progress: null,
        finished: { jobId: "old", rows: oldRows, cancelled: outcome === "cancelled", error: outcome === "error" ? "obsolete failure" : null },
      });
      await vi.advanceTimersByTimeAsync(200);
      expect(await old).toBe(false);
      expect(api.startSearch).toHaveBeenCalledTimes(2);
      expect(useDesktopStore.getState()).toMatchObject({
        isSearching: true, activeJobId: "new", progress: null, error: null, notices: [],
      });
      expect(useDesktopStore.getState().rows).toBe(retainedRows);

      completeNew({ progress: null, finished: { jobId: "new", rows: newRows, cancelled: false, error: null } });
      expect(await replacement).toBe(true);
      expect(useDesktopStore.getState().rows).toBe(newRows);
      expect(useDesktopStore.getState()).toMatchObject({ isSearching: false, activeJobId: null, error: null });
    },
  );
}
