import { beforeEach, describe, expect, it } from "vitest";
import { MAX_COMPARE_PINS, readCompareBench } from "./compare-bench";
import { solvedBuild } from "./test-fixtures";
import type { CatalogDto } from "./types";

class MemoryStorage implements Storage {
  private values = new Map<string, string>();
  get length() { return this.values.size; }
  clear() { this.values.clear(); }
  getItem(key: string) { return this.values.get(key) ?? null; }
  key(index: number) { return Array.from(this.values.keys())[index] ?? null; }
  removeItem(key: string) { this.values.delete(key); }
  setItem(key: string, value: string) { this.values.set(key, value); }
}

const catalog = {
  dataManifest: {
    profile: { id: "vanilla" }, datasetVersion: "d", schemaVersion: 1, modelVersion: "m",
    rules: { standardMaxUpgrade: 25, somberMaxUpgrade: 10 },
  },
} as unknown as CatalogDto;
const KEY = "tarnisheds-arsenal.compareBench.v1.vanilla";

function store(rows: unknown[], versions = { datasetVersion: "d", schemaVersion: 1, modelVersion: "m" }) {
  localStorage.setItem(KEY, JSON.stringify({ version: 1, ...versions, rows }));
}

describe("readCompareBench", () => {
  beforeEach(() => {
    Object.defineProperty(globalThis, "localStorage", { configurable: true, value: new MemoryStorage() });
  });

  it("restores current pins without a notice", () => {
    store([solvedBuild(), solvedBuild({ weaponId: 2, weaponName: "Moonveil" })]);
    const { rows, staleRows, notices } = readCompareBench(catalog);
    expect(rows.map((row) => row.weaponName)).toEqual(["Uchigatana", "Moonveil"]);
    expect(staleRows).toEqual([]);
    expect(notices).toEqual([]);
  });

  it("keeps the first pins of an oversized list and says the rest were dropped", () => {
    store(Array.from({ length: 50_000 }, (_, index) => solvedBuild({ weaponId: index + 1, weaponName: `Weapon ${index + 1}` })));
    const { rows, notices } = readCompareBench(catalog);
    expect(rows).toHaveLength(MAX_COMPARE_PINS);
    expect(rows[0].weaponName).toBe("Weapon 1");
    expect(notices).toEqual([expect.objectContaining({ tone: "warning", message: expect.stringContaining("49992 saved pins over that limit were not restored") })]);
  });

  it("validates only the pins it can keep", () => {
    // A malformed row past the limit is never inspected, so it cannot raise the invalid-data notice.
    store([...Array.from({ length: MAX_COMPARE_PINS }, (_, index) => solvedBuild({ weaponId: index + 1 })), { broken: true }]);
    const { rows, notices } = readCompareBench(catalog);
    expect(rows).toHaveLength(MAX_COMPARE_PINS);
    expect(notices).toHaveLength(1);
    expect(notices[0].message).toContain("1 saved pin over that limit was not restored");
  });

  it("discards invalid rows with a notice", () => {
    store([solvedBuild(), { broken: true }]);
    const { rows, notices } = readCompareBench(catalog);
    expect(rows).toHaveLength(1);
    expect(notices[0].message).toContain("invalid");
  });

  it("returns pins saved on other data as stale rows", () => {
    store([solvedBuild()], { datasetVersion: "old", schemaVersion: 1, modelVersion: "m" });
    const { rows, staleRows } = readCompareBench(catalog);
    expect(rows).toEqual([]);
    expect(staleRows).toHaveLength(1);
  });
});
