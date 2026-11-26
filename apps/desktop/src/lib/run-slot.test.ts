import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createNativeJobQueue, isCancelled } from "./native-jobs";
import { createRunSlot } from "./run-slot";
import { useDesktopStore } from "./state";

type Status = { progress: { jobId: string } | null; finished: { jobId: string; cancelled: boolean; error: string | null } | null };

const status = vi.fn<(jobId: string) => Promise<Status | null>>();
const cancel = vi.fn<(jobId: string) => Promise<boolean>>();

function pathSlot() {
  return createRunSlot<Status>({
    queue: createNativeJobQueue<Status>(status, cancel),
    read: (state) => ({ busy: state.isPathBusy, generation: state.pathGeneration, signature: state.activePathSignature, jobId: state.activePathJobId }),
    begin: (signature) => useDesktopStore.getState().beginPath(signature),
    setJobId: (jobId) => useDesktopStore.getState().setActivePathJobId(jobId),
    end: () => useDesktopStore.setState({ isPathBusy: false, activePathJobId: null, pathProgress: null }),
    usable: (state) => !state.resultsStale,
  });
}

const running = (jobId: string): Status => ({ progress: { jobId }, finished: null });
const finished = (jobId: string, cancelled = false): Status => ({ progress: null, finished: { jobId, cancelled, error: null } });

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  cancel.mockResolvedValue(true);
  useDesktopStore.setState({ isPathBusy: false, pathGeneration: 0, activePathSignature: null, activePathJobId: null, resultsStale: false });
});
afterEach(() => vi.useRealTimers());

describe("run slot", () => {
  it("passes status on only while the reply's job is the slot's current job", async () => {
    let done = false;
    status.mockImplementation(async (jobId) => (done ? finished(jobId) : running(jobId)));
    const slot = pathSlot();
    const token = slot.begin("inputs");
    const onStatus = vi.fn();
    const result = slot.run(token, async () => ({ jobId: "a" }), onStatus);
    await vi.advanceTimersByTimeAsync(0);
    expect(useDesktopStore.getState().activePathJobId).toBe("a");
    expect(onStatus).toHaveBeenCalledWith(running("a"), "a");

    onStatus.mockClear();
    useDesktopStore.getState().setActivePathJobId("other");
    await vi.advanceTimersByTimeAsync(200);
    expect(onStatus).not.toHaveBeenCalled();

    done = true;
    await vi.advanceTimersByTimeAsync(200);
    const reply = await result;
    expect(slot.owns(useDesktopStore.getState(), token)).toBe(true);
    expect(slot.accepts(useDesktopStore.getState(), token, reply.jobId)).toBe(false);
  });

  it("cancels the native job once through the queue when a new run takes the slot", async () => {
    let cancelled = false;
    cancel.mockImplementation(async () => { cancelled = true; return true; });
    status.mockImplementation(async (jobId) => (cancelled ? finished(jobId, true) : running(jobId)));
    const slot = pathSlot();
    const token = slot.begin("inputs");
    const result = slot.run(token, async () => ({ jobId: "a" })).catch(error => error);
    await vi.advanceTimersByTimeAsync(0);

    slot.begin("newer inputs");
    useDesktopStore.getState().setActivePathJobId(null);
    await vi.advanceTimersByTimeAsync(200);
    expect(isCancelled(await result)).toBe(true);
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(cancel).toHaveBeenCalledWith("a");
    expect(slot.owns(useDesktopStore.getState(), token)).toBe(false);
  });

  it("cancels the current run through the queue on Stop", async () => {
    let cancelled = false;
    cancel.mockImplementation(async () => { cancelled = true; return true; });
    status.mockImplementation(async (jobId) => (cancelled ? finished(jobId, true) : running(jobId)));
    const slot = pathSlot();
    const token = slot.begin("inputs");
    const result = slot.run(token, async () => ({ jobId: "a" })).catch(error => error);
    await vi.advanceTimersByTimeAsync(0);

    slot.cancel();
    await vi.advanceTimersByTimeAsync(200);
    expect(isCancelled(await result)).toBe(true);
    expect(cancel).toHaveBeenCalledWith("a");
    slot.end(token);
    expect(useDesktopStore.getState()).toMatchObject({ isPathBusy: false, activePathJobId: null });
  });

  it("cancels a job that started after its run went stale and never records it", async () => {
    let release!: (value: { jobId: string }) => void;
    let cancelled = false;
    cancel.mockImplementation(async () => { cancelled = true; return true; });
    status.mockImplementation(async (jobId) => (cancelled ? finished(jobId, true) : running(jobId)));
    const slot = pathSlot();
    const token = slot.begin("inputs");
    const result = slot.run(token, () => new Promise(resolve => { release = resolve; })).catch(error => error);
    await vi.advanceTimersByTimeAsync(0);

    useDesktopStore.setState({ resultsStale: true });
    release({ jobId: "late" });
    await vi.advanceTimersByTimeAsync(200);
    expect(isCancelled(await result)).toBe(true);
    expect(cancel).toHaveBeenCalledWith("late");
    expect(useDesktopStore.getState().activePathJobId).toBeNull();
  });
});
