import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "./api";
import { defaultRequest, useDesktopStore } from "./state";
import { runSearchFromStore, runSearchRequestForRows } from "./workflows";

vi.mock("./api", () => ({ api: { startSearch: vi.fn(), searchStatus: vi.fn(), cancelSearch: vi.fn() } }));

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  useDesktopStore.setState({ isExporting: false, isSearching: false, activeJobId: null, activeSearchSignature: null, error: null });
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
