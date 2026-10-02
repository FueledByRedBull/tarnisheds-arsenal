import { expect, test, type Page } from "@playwright/test";
import { openEditor } from "./editors";

async function restore(page: Page, fixed: boolean, immediateSearch: boolean, explicit = false) {
  await page.goto("/");
  await page.getByText("Snapshot loaded", { exact: true }).waitFor();
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
  await page.evaluate(async ({ fixed, immediateSearch, explicit }) => {
    const { api } = await import("/src/lib/api.ts");
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const { clearAnalysisCaches } = await import("/src/lib/analysis-cache.ts");
    const state = useDesktopStore.getState();
    const row = fixed ? state.rows[3] : state.rows[0];
    const manifest = state.catalog!.dataManifest;
    const preset = { version: 2 as const, id: "restored-selection", name: "Restored skill policy", profileId: "vanilla",
      request: { ...state.request, weaponName: row.weaponName, affinity: row.affinity, aowName: explicit ? row.aowName : null },
      selectedBuild: row, compareTarget: null, compareBench: [], createdAt: "2026-09-29", updatedAt: "2026-09-29",
      dataVersion: `${manifest.profile.id}:${manifest.schemaVersion}:${manifest.datasetVersion}:${manifest.modelVersion}` };
    const profile = api.weaponProfile;
    clearAnalysisCaches();
    api.weaponProfile = async (...args) => ({ ...await profile(...args),
      canChangeAow: !fixed, nativeSkillName: fixed ? row.aowName : "Unsheathe",
      compatibleAows: fixed ? [row.aowName!] : ["Unsheathe", row.aowName!],
    });
    const probe: any = { aborted: false, completed: false, searches: [], expected: preset.request.aowName, row };
    api.solveBuild = (_base, _name, _affinity, _skill, signal) => new Promise((resolve, reject) => {
      probe.finish = () => resolve(row);
      signal.addEventListener("abort", () => { probe.aborted = true; reject(new Error("cancelled")); }, { once: true });
    });
    api.startSearch = async request => { probe.searches.push(request); return { jobId: "restored-search" }; };
    api.searchStatus = async () => ({ progress: null, finished: null });
    api.cancelSearch = async () => true;
    Object.assign(window, { restoredSelectionProbe: probe });
    void state.loadBuildPreset(preset).then(result => { probe.completed = true; probe.loaded = result !== null; });
    if (immediateSearch) document.querySelector<HTMLButtonElement>(".search-button")!.click();
  }, { fixed, immediateSearch, explicit });
  await openEditor(page, "Loadout");
  await expect(page.getByRole("combobox", { name: fixed ? "AoW (fixed)" : "AoW", exact: true })).toHaveValue(
    fixed ? "White Light Charge" : explicit ? "Seppuku" : "Automatic (best legal skill)",
  );
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}

for (const fixed of [false, true]) {
  test(`restored ${fixed ? "fixed-native" : "transferable"} Automatic survives metadata and verification`, async ({ page }) => {
    await restore(page, fixed, false);
    expect(await page.evaluate(() => (window as any).restoredSelectionProbe.aborted)).toBe(false);
    await page.evaluate(() => (window as any).restoredSelectionProbe.finish());
    await expect(page.getByText(/saved results verified on current data/)).toBeVisible();
    expect(await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      return { skill: useDesktopStore.getState().request.aowName, selected: useDesktopStore.getState().selected?.weaponName };
    })).toEqual({ skill: null, selected: await page.evaluate(() => (window as any).restoredSelectionProbe.row.weaponName) });
  });

  test(`immediate Search preserves restored ${fixed ? "fixed-native" : "transferable"} Automatic`, async ({ page }) => {
    await restore(page, fixed, true);
    await expect.poll(() => page.evaluate(() => (window as any).restoredSelectionProbe.searches.length)).toBe(1);
    expect(await page.evaluate(() => (window as any).restoredSelectionProbe.searches[0].aowName)).toBeNull();
  });
}

test("a restored explicit skill remains explicit", async ({ page }) => {
  await restore(page, false, false, true);
  expect(await page.evaluate(() => (window as any).restoredSelectionProbe.aborted)).toBe(false);
  await page.evaluate(() => (window as any).restoredSelectionProbe.finish());
  await expect(page.getByText(/saved results verified on current data/)).toBeVisible();
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).restoredSelectionProbe.searches.length)).toBe(1);
  expect(await page.evaluate(() => (window as any).restoredSelectionProbe.searches[0].aowName)).toBe("Seppuku");
});

test("manual weapon selection after restoration still dispatches its native default", async ({ page }) => {
  await restore(page, false, false);
  await page.evaluate(async () => {
    (window as any).restoredSelectionProbe.finish();
    const { api } = await import("/src/lib/api.ts");
    const profile = api.weaponProfile;
    api.weaponProfile = async (...args) => ({ ...await profile(...args),
      nativeSkillName: "Stamp (Upward Cut)", compatibleAows: ["Stamp (Upward Cut)"],
    });
  });
  await expect(page.getByText(/saved results verified on current data/)).toBeVisible();
  await (await openEditor(page, "Loadout")).getByRole("combobox", { name: "Weapon", exact: true }).fill("Zweihander");
  await page.evaluate(() => {
    document.querySelector<HTMLInputElement>('input[aria-label="Weapon"]')!.blur();
    document.querySelector<HTMLButtonElement>(".search-button")!.click();
  });
  await expect.poll(() => page.evaluate(() => (window as any).restoredSelectionProbe.searches.length)).toBe(1);
  expect(await page.evaluate(() => (window as any).restoredSelectionProbe.searches[0].aowName)).toBe("Stamp (Upward Cut)");
});
