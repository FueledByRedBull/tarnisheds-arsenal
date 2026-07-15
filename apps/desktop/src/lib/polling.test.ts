import { afterEach, describe, expect, it, vi } from "vitest";
import { INITIAL_POLL_DELAY_MS, MAX_POLL_DELAY_MS, nextPollDelay, progressSignature, startAdaptivePolling } from "./polling";

describe("adaptive polling", () => {
  afterEach(() => vi.useRealTimers());
  it("backs off unchanged progress and caps at one second", () => {
    let delay = INITIAL_POLL_DELAY_MS;
    for (let index = 0; index < 20; index += 1) {
      delay = nextPollDelay(delay, false);
    }
    expect(delay).toBe(MAX_POLL_DELAY_MS);
  });

  it("resets immediately when progress changes", () => {
    expect(nextPollDelay(MAX_POLL_DELAY_MS, true)).toBe(INITIAL_POLL_DELAY_MS);
  });

  it("uses value signatures instead of object identity", () => {
    expect(progressSignature({ checked: 10, total: 100 })).toBe(
      progressSignature({ checked: 10, total: 100 }),
    );
    expect(progressSignature({ checked: 11, total: 100 })).not.toBe(
      progressSignature({ checked: 10, total: 100 }),
    );
  });

  it("never overlaps status calls that take longer than the initial interval", async () => {
    vi.useFakeTimers();
    let calls = 0;
    let inFlight = 0;
    let maxInFlight = 0;
    const polling = startAdaptivePolling({
      poll: async () => {
        calls += 1;
        inFlight += 1;
        maxInFlight = Math.max(maxInFlight, inFlight);
        await new Promise((resolve) => setTimeout(resolve, 500));
        inFlight -= 1;
        return { progress: calls };
      },
      progressKey: (status) => String(status.progress),
      onStatus: () => calls >= 2,
      onMissing: () => undefined,
      onError: (error) => { throw error; },
    });

    await vi.advanceTimersByTimeAsync(499);
    expect(calls).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    await vi.advanceTimersByTimeAsync(INITIAL_POLL_DELAY_MS - 1);
    expect(calls).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(calls).toBe(2);
    expect(maxInFlight).toBe(1);
    polling.stop();
  });

  it("ignores a late status after polling is stopped", async () => {
    let resolveStatus!: (value: { progress: number }) => void;
    const onStatus = vi.fn(() => false);
    const polling = startAdaptivePolling({
      poll: () => new Promise<{ progress: number }>((resolve) => { resolveStatus = resolve; }),
      progressKey: (status) => String(status.progress),
      onStatus,
      onMissing: () => undefined,
      onError: (error) => { throw error; },
    });

    polling.stop();
    resolveStatus({ progress: 1 });
    await Promise.resolve();
    expect(onStatus).not.toHaveBeenCalled();
  });
});
