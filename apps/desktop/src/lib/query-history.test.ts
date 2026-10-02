import { afterEach, beforeEach, expect, it } from "vitest";
import { describeStep, redoQuery, trackQueryHistory, undoQuery, useQueryHistory, withoutHistory } from "./query-history";
import { defaultRequest, useDesktopStore } from "./state";
import type { CatalogDto, OptimizeRequestDto } from "./types";

let now = 0;
let stop: () => void = () => {};

beforeEach(() => {
  useDesktopStore.setState({ request: defaultRequest, lockedStatMode: false, catalog: null, rows: [] });
  useQueryHistory.setState({ past: [], future: [], announcement: "" });
  now = 1_000;
  stop = trackQueryHistory(() => now);
});

afterEach(() => stop());

function edit(patch: Partial<OptimizeRequestDto>, after = 1_000) {
  now += after;
  useDesktopStore.getState().patchRequest(patch);
}

const request = () => useDesktopStore.getState().request;
const history = () => useQueryHistory.getState();

it("undoes and redoes each edit and names it", () => {
  edit({ strStat: 40 });
  edit({ objective: "max_ar_plus_bleed" });
  expect(history().past.map(describeStep)).toEqual(["STR 12 → 40", "Objective Max AR → Bleed, then AR"]);

  undoQuery();
  expect(request().objective).toBe("max_ar");
  expect(history().announcement).toBe("Undid Objective Max AR → Bleed, then AR");
  undoQuery();
  expect(request().strStat).toBe(12);
  expect(undoQuery()).toBeNull();

  redoQuery();
  expect(request().strStat).toBe(40);
  expect(history().announcement).toBe("Redid STR 12 → 40");
  edit({ dex: 20 });
  expect(history().future).toEqual([]);
  expect(history().past.map(describeStep)).toEqual(["STR 12 → 40", "DEX 15 → 20"]);
});

it("merges quick edits, including a lock and its mode, into one step", () => {
  edit({ lockDex: 30 });
  now += 10;
  useDesktopStore.getState().setLockedStatMode(true);
  expect(history().past).toHaveLength(1);
  expect(describeStep(history().past[0])).toBe("DEX lock, Locked stats on");
  undoQuery();
  expect(request().lockDex).toBeNull();
  expect(useDesktopStore.getState().lockedStatMode).toBe(false);
});

it("keeps different quick edits as separate steps but merges repeats of one field", () => {
  edit({ strStat: 40 });
  edit({ objective: "max_ar_plus_bleed" }, 200);
  expect(history().past).toHaveLength(2);
  edit({ topK: 10 }, 1_000);
  edit({ topK: 5 }, 200);
  expect(history().past.map(describeStep).at(-1)).toBe("Top results 25 → 5");
});

it("folds automatic follow-ups into the edit that caused them", () => {
  edit({ weaponName: "Uchigatana" });
  now += 5_000;
  withoutHistory(() => useDesktopStore.getState().patchRequest({ affinity: "Keen" }));
  expect(history().past).toHaveLength(1);
  expect(history().past[0].after.request.affinity).toBe("Keen");
  undoQuery();
  expect(request()).toMatchObject({ weaponName: null, affinity: null });
  redoQuery();
  expect(request()).toMatchObject({ weaponName: "Uchigatana", affinity: "Keen" });
});

it("drops edits that return to the start and forgets history for a new catalog", () => {
  edit({ strStat: 40 });
  edit({ strStat: 12 }, 10);
  expect(history().past).toEqual([]);
  edit({ strStat: 30 });
  expect(history().past).toHaveLength(1);
  useDesktopStore.setState({ catalog: {} as CatalogDto });
  expect(history().past).toEqual([]);
});

it("describes many changes by name", () => {
  edit({ className: "Vagabond", vig: 15, mnd: 10, end: 11, strStat: 14 });
  expect(describeStep(history().past[0])).toBe("Class, VIG, MND and 2 more");
});
