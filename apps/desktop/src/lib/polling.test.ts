import { describe, expect, it } from "vitest";
import { INITIAL_POLL_DELAY_MS, MAX_POLL_DELAY_MS, nextPollDelay, progressSignature } from "./polling";

describe("adaptive polling", () => {
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
});
