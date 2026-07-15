import { beforeEach, describe, expect, it } from "vitest";

import { defaultRequest, useDesktopStore } from "./state";
import type { SolvedBuildDto } from "./types";

const row: SolvedBuildDto = {
  weaponId: 1,
  weaponName: "Uchigatana",
  affinity: "Keen",
  isSomber: false,
  upgrade: 25,
  stats: { strStat: 18, dex: 40, intStat: 9, fai: 8, arc: 8 },
  ar: { physical: 500, magic: 0, fire: 0, lightning: 0, holy: 0, total: 500 },
  aowId: 100,
  aowName: "Unsheathe",
  bleedBuildup: 45,
  bleedBuildupAdd: 0,
  frostBuildup: 0,
  poisonBuildup: 0,
  scarletRotBuildup: 0,
  aowFirstHitDamage: 300,
  aowFullSequenceDamage: 300,
  aowRoute: null,
  score: 500,
};

describe("desktop result lifecycle", () => {
  beforeEach(() => {
    useDesktopStore.setState({
      catalog: null,
      catalogStatus: "loading",
      catalogError: null,
      request: { ...defaultRequest },
      rows: [],
      resultsStale: false,
      selected: null,
      compareTarget: null,
      selectedFingerprint: null,
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: [],
      isSearching: false,
    });
  });

  it("retains previous rows and labels them stale when result inputs change", () => {
    const state = useDesktopStore.getState();
    state.setRows([row]);
    state.setCompareTarget(row);
    state.patchRequest({ objective: "max_physical_ar" });

    const changed = useDesktopStore.getState();
    expect(changed.rows).toEqual([row]);
    expect(changed.selected).toEqual(row);
    expect(changed.compareTarget).toBeNull();
    expect(changed.resultsStale).toBe(true);
  });

  it("marks replacement rows current after a successful search", () => {
    const state = useDesktopStore.getState();
    state.setRows([row]);
    state.patchRequest({ twoHanding: true });
    expect(useDesktopStore.getState().resultsStale).toBe(true);

    useDesktopStore.getState().setRows([{ ...row, score: 510 }]);
    expect(useDesktopStore.getState().resultsStale).toBe(false);
  });

  it("records recoverable catalog loading failures", () => {
    const state = useDesktopStore.getState();
    state.setCatalogFailure("manifest checksum mismatch");
    expect(useDesktopStore.getState()).toMatchObject({
      catalogStatus: "error",
      catalogError: "manifest checksum mismatch",
    });

    useDesktopStore.getState().setCatalogLoading();
    expect(useDesktopStore.getState()).toMatchObject({
      catalogStatus: "loading",
      catalogError: null,
    });
  });

  it("keeps Paths and Affinity Watch horizons independent", () => {
    const state = useDesktopStore.getState();
    state.setPathHorizon(25);
    state.setAffinityHorizon(80);

    expect(useDesktopStore.getState().pathHorizon).toBe(25);
    expect(useDesktopStore.getState().affinityHorizon).toBe(80);

    useDesktopStore.getState().setPathHorizon(12);
    expect(useDesktopStore.getState().affinityHorizon).toBe(80);
  });
});
