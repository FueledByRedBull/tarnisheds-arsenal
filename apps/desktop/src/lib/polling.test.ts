import { describe, expect, it } from "vitest";
import { INITIAL_POLL_DELAY_MS, MAX_POLL_DELAY_MS, PROGRESS_POLL_DELAY_MS, nextPollDelay, progressSignature } from "./polling";

describe("adaptive polling", () => {
  it("starts with short checks and caps unchanged progress at 50 ms", () => {
    const delays = [INITIAL_POLL_DELAY_MS];
    for (let index = 0; index < 8; index += 1) delays.push(nextPollDelay(delays.at(-1)!, false));
    expect(delays).toEqual([8, 12, 18, 27, 41, 50, 50, 50, 50]);
    expect(MAX_POLL_DELAY_MS).toBe(50);
  });

  it("checks again soon after progress changes", () => {
    expect(nextPollDelay(MAX_POLL_DELAY_MS, true)).toBe(PROGRESS_POLL_DELAY_MS);
    expect(PROGRESS_POLL_DELAY_MS).toBeLessThan(MAX_POLL_DELAY_MS);
  });

  it("uses value signatures instead of object identity", () => {
    expect(progressSignature({ checked: 10, total: 100 })).toBe(
      progressSignature({ checked: 10, total: 100 }),
    );
    expect(progressSignature({ checked: 11, total: 100 })).not.toBe(
      progressSignature({ checked: 10, total: 100 }),
    );
  });
});
