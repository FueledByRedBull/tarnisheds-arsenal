import { describe, expect, it } from "vitest";
import { runSlice, warmupSteps } from "./font-warmup";

describe("font warm-up", () => {
  it("covers every weight and size with and without tabular figures, once each", () => {
    const steps = warmupSteps();
    expect(new Set(steps.map((step) => `${step.weight}/${step.size}/${step.tabular}`)).size).toBe(steps.length);
    expect(steps).toHaveLength(5 * 6 * 2);
    expect(steps.filter((step) => step.tabular)).toHaveLength(steps.length / 2);
  });

  it("stops a slice once its budget is spent and resumes where it stopped", () => {
    const steps = warmupSteps();
    let clock = 0;
    const ran: number[] = [];
    // Each step costs 1 ms.
    const run = (step: (typeof steps)[number]) => { ran.push(steps.indexOf(step)); clock += 1; };
    const now = () => clock;

    let next = runSlice(steps, 0, run, 3, now);
    expect(next).toBe(3);
    next = runSlice(steps, next, run, 3, now);
    expect(next).toBe(6);
    expect(ran).toEqual([0, 1, 2, 3, 4, 5]);
  });

  it("makes progress even when there is no idle time", () => {
    const steps = warmupSteps();
    const ran: number[] = [];
    const next = runSlice(steps, 10, (step) => ran.push(steps.indexOf(step)), 0, () => 0);
    expect(next).toBe(11);
    expect(ran).toEqual([10]);
  });

  it("finishes at the end of the list without running past it", () => {
    const steps = warmupSteps();
    let count = 0;
    const next = runSlice(steps, steps.length - 2, () => { count += 1; }, 1_000, () => 0);
    expect(next).toBe(steps.length);
    expect(count).toBe(2);
    expect(runSlice(steps, steps.length, () => { count += 1; }, 3)).toBe(steps.length);
    expect(count).toBe(2);
  });
});
