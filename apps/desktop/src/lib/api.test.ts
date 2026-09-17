import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { api } from "./api";
import { defaultRequest } from "./state";
import type { SolvedBuildDto } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.resetAllMocks(); });

it("serializes solve and upgrade calculations until cancelled native work finishes", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
  let finished = false;
  const calls: string[] = [];
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    calls.push(command);
    if (command === "start_solve_build") return { jobId: "solve" };
    if (command === "start_upgrade_series") return { jobId: "series" };
    if (command === "cancel_analysis") return true;
    if (command === "get_analysis_status") {
      const series = (args as { jobId: string }).jobId === "series";
      return { finished: !series && !finished ? null : {
        jobId: series ? "series" : "solve", kind: series ? "upgrade_series" : "solve_build",
        cancelled: !series, error: null, result: null, points: [{ upgrade: 0, metric: 10 }],
      } };
    }
    throw new Error(`Unexpected command ${command}`);
  });
  const controller = new AbortController();
  const solve = api.solveBuild(defaultRequest, "Uchigatana", "Keen", null, controller.signal).catch(error => error);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  const series = api.buildUpgradeSeries(defaultRequest, {} as SolvedBuildDto, 25);
  await vi.advanceTimersByTimeAsync(0);
  expect(calls).toContain("cancel_analysis");
  expect(calls).not.toContain("start_upgrade_series");
  finished = true;
  await vi.advanceTimersByTimeAsync(200);
  expect(await solve).toMatchObject({ name: "AbortError" });
  expect(await series).toEqual([{ upgrade: 0, metric: 10 }]);
});

it("does not start native calculations for an already aborted caller", async () => {
  const controller = new AbortController();
  controller.abort();
  await expect(api.solveBuild(defaultRequest, "Uchigatana", null, null, controller.signal)).rejects.toMatchObject({ name: "AbortError" });
  expect(invoke).not.toHaveBeenCalled();
});

it("queues the frontier with other analyses and forwards the fixed context", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
  let released = false;
  const points = [{ minimumArLossBps: 101, arLossPercent: 1.001 }];
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "start_upgrade_series") return { jobId: "series" };
    if (command === "start_ar_bleed_frontier") return { jobId: "frontier" };
    if (command === "get_analysis_status") {
      const frontier = (args as { jobId: string }).jobId === "frontier";
      return { finished: !frontier && !released ? null : {
        jobId: frontier ? "frontier" : "series",
        kind: frontier ? "ar_bleed_frontier" : "upgrade_series",
        cancelled: false, error: null, result: null, points: [], frontier: points,
      } };
    }
    throw new Error(`Unexpected command ${command}`);
  });
  const solved = { weaponId: 100, upgrade: 25 } as SolvedBuildDto;
  const series = api.buildUpgradeSeries(defaultRequest, solved, 25);
  const frontier = api.arBleedFrontier(defaultRequest, solved);
  await vi.advanceTimersByTimeAsync(0);
  expect(vi.mocked(invoke).mock.calls.some(([name]) => name === "start_ar_bleed_frontier")).toBe(false);
  released = true;
  await vi.advanceTimersByTimeAsync(200);
  await series;
  expect(await frontier).toEqual(points);
  expect(invoke).toHaveBeenCalledWith("start_ar_bleed_frontier", { request: { base: defaultRequest, solved } });
  const controller = new AbortController();
  controller.abort();
  await expect(api.arBleedFrontier(defaultRequest, solved, controller.signal)).rejects.toMatchObject({ name: "AbortError" });
});
