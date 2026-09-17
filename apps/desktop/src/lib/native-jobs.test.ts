import { afterEach, expect, it, vi } from "vitest";
import { createNativeJobQueue } from "./native-jobs";

afterEach(() => vi.useRealTimers());

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
