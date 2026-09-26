import { afterEach, expect, it, vi } from "vitest";
import { createNativeJobQueue } from "./native-jobs";

afterEach(() => vi.useRealTimers());

it("stops polling a worker failure and permits the next job", async () => {
  vi.useFakeTimers();
  type Status = { finished: { jobId: string; cancelled: boolean; error: string | null } | null };
  const status = vi.fn()
    .mockResolvedValueOnce({ finished: null })
    .mockResolvedValueOnce({ finished: {
      jobId: "failed", cancelled: false,
      error: "Calculation worker stopped unexpectedly. Retry the operation.",
    } })
    .mockResolvedValueOnce({ finished: { jobId: "next", cancelled: false, error: null } });
  const cancel = vi.fn();
  const start = vi.fn().mockResolvedValueOnce({ jobId: "failed" }).mockResolvedValueOnce({ jobId: "next" });
  const queue = createNativeJobQueue<Status>(status, cancel);
  const failed = queue(start).catch(error => error);
  await vi.advanceTimersByTimeAsync(1_000);
  expect(await failed).toMatchObject({ message: "Calculation worker stopped unexpectedly. Retry the operation." });
  await vi.advanceTimersByTimeAsync(10_000);
  expect(status).toHaveBeenCalledTimes(2);
  expect(cancel).not.toHaveBeenCalled();
  await expect(queue(start)).resolves.toMatchObject({ jobId: "next" });
  expect(status).toHaveBeenCalledTimes(3);
  expect(start).toHaveBeenCalledTimes(2);
});

it("bounds cancellation while a status reply is stalled, then reconciles that same reply", async () => {
  vi.useFakeTimers();
  type Status = { finished: { jobId: string; cancelled: boolean; error: null } | null };
  let complete!: (status: Status) => void;
  const status = vi.fn().mockImplementationOnce(() => new Promise<Status>(resolve => { complete = resolve; }))
    .mockResolvedValue({ finished: { jobId: "new", cancelled: false, error: null } });
  const queue = createNativeJobQueue<Status>(status, vi.fn().mockResolvedValue(true));
  const start = vi.fn().mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "new" });
  const controller = new AbortController();
  const old = queue(start, controller.signal).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  await vi.advanceTimersByTimeAsync(3_000);
  expect(await old).toMatchObject({ message: expect.stringContaining("state is unknown") });
  const replacement = queue(start);
  await vi.advanceTimersByTimeAsync(0);
  expect(start).toHaveBeenCalledTimes(1);
  expect(status).toHaveBeenCalledTimes(1);
  complete({ finished: { jobId: "old", cancelled: true, error: null } });
  await expect(replacement).resolves.toMatchObject({ jobId: "new" });
  expect(start).toHaveBeenCalledTimes(2);
});

it("rejects a different job's terminal reply without releasing the current worker", async () => {
  vi.useFakeTimers();
  type Status = { finished: { jobId: string; cancelled: boolean; error: null } | null };
  let oldFinished = false;
  const status = vi.fn().mockResolvedValueOnce({ finished: { jobId: "unrelated", cancelled: false, error: null } })
    .mockImplementation(async (jobId: string) => ({
      finished: jobId === "new" || oldFinished ? { jobId, cancelled: false, error: null } : null,
    }));
  const cancel = vi.fn().mockResolvedValue(true);
  const start = vi.fn().mockResolvedValueOnce({ jobId: "old" }).mockResolvedValue({ jobId: "new" });
  const queue = createNativeJobQueue<Status>(status, cancel);
  const old = queue(start).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  expect(await old).toMatchObject({ message: "Native status returned a different job." });
  const replacement = queue(start);
  await vi.advanceTimersByTimeAsync(200);
  expect(start).toHaveBeenCalledTimes(1);
  expect(cancel).toHaveBeenCalledExactlyOnceWith("old");
  oldFinished = true;
  await vi.advanceTimersByTimeAsync(1_000);
  await expect(replacement).resolves.toMatchObject({ jobId: "new" });
  expect(start).toHaveBeenCalledTimes(2);
});
