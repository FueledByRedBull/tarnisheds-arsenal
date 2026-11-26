import { afterEach, describe, expect, it, vi } from "vitest";

import { whenIdle } from "./idle";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("idle scheduling", () => {
  it("falls back to a timer that reports the given budget when idle callbacks are missing", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {});
    const callback = vi.fn();
    whenIdle(callback, 100, 8);
    vi.advanceTimersByTime(99);
    expect(callback).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(callback).toHaveBeenCalledOnce();
    expect(callback.mock.calls[0][0].timeRemaining()).toBe(8);
  });

  it("cancels the fallback timer", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {});
    const callback = vi.fn();
    whenIdle(callback, 100)();
    vi.advanceTimersByTime(200);
    expect(callback).not.toHaveBeenCalled();
  });

  it("uses idle callbacks with a timeout when the browser has them", () => {
    const requestIdleCallback = vi.fn(() => 7);
    const cancelIdleCallback = vi.fn();
    vi.stubGlobal("window", { requestIdleCallback, cancelIdleCallback });
    const callback = vi.fn();
    whenIdle(callback, 100)();
    expect(requestIdleCallback).toHaveBeenCalledWith(callback, { timeout: 2_000 });
    expect(cancelIdleCallback).toHaveBeenCalledWith(7);
  });
});
