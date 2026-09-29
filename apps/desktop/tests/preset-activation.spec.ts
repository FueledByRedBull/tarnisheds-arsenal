import { expect, test, type Page } from "@playwright/test";

async function saved(page: Page, results = true) {
  await page.goto("/");
  await page.getByText("Snapshot loaded", { exact: true }).waitFor();
  if (results) {
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
  }
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  return page.evaluate(() => {
    const key = Object.keys(localStorage).find(key => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
    return JSON.parse(localStorage.getItem(key)!);
  });
}

async function importPreset(page: Page, preset: any) {
  await page.locator("#saved-builds-panel textarea").fill(JSON.stringify(preset));
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Saved", exact: true })).not.toHaveValue("");
}

for (const [className, stale, migrate] of [["Unknown", false, false], ["Unknown", true, false], ["Obsolete class", true, true]] as const) {
  test(`rejects ${className} ${stale ? "old" : "current"} activation${migrate ? " migration" : ""} without losing the workspace`, async ({ page }) => {
    const preset = await saved(page);
    preset.id = `invalid-${stale}-${migrate}`;
    preset.name = "Invalid class archive";
    preset.request.className = className;
    if (stale) preset.dataVersion = "vanilla:4:old:old";
    await importPreset(page, preset);
    const before = await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const state = useDesktopStore.getState();
      return { request: state.request, rows: state.rows, pins: state.compareBench,
        pathGeneration: state.pathGeneration, searchGeneration: state.searchGeneration, storage: { ...localStorage } };
    });
    await page.getByRole("button", { name: migrate ? "Migrate data" : stale ? "Load inputs only" : "Load", exact: true }).click();
    await expect(page.locator('.error-strip[role="alert"]')).toContainText("Starting class");
    await expect(page.getByRole("grid", { name: "Ranked builds" })).toBeVisible();
    expect(await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const state = useDesktopStore.getState();
      return { request: state.request, rows: state.rows, pins: state.compareBench,
        pathGeneration: state.pathGeneration, searchGeneration: state.searchGeneration, storage: { ...localStorage } };
    })).toEqual(before);
  });
}

test("canonicalizes an unambiguous imported class in a mounted workspace", async ({ page }) => {
  const preset = await saved(page, false);
  preset.id = "lowercase";
  preset.request.className = "samurai";
  await importPreset(page, preset);
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Class", exact: true })).toHaveValue("Samurai");
  await expect(page.getByText(/saved results verified on current data/)).toBeVisible();
});

for (const mutation of ["total", "route", "identity", "upgrade"] as const) {
  test(`verifies external ${mutation} values instead of trusting a matching version`, async ({ page }) => {
    const preset = await saved(page);
    preset.id = `external-${mutation}`;
    preset.name = `External ${mutation}`;
    if (mutation === "total") preset.selectedBuild.ar.total = 999999;
    if (mutation === "route") {
      preset.selectedBuild.aowFirstHitDamage = 999999;
      preset.selectedBuild.aowFullSequenceDamage = 999999;
      preset.selectedBuild.aowRoute = { routeId: "forged", routeLabel: "Forged", routePriority: 0,
        buffActivationActionId: null, actions: [], firstHitDamage: 999999,
        totalDamage: preset.selectedBuild.ar, totalPoiseDamage: 999999, totalStaminaCost: 0,
        totalStatusBuildup: { bleed: 0, frost: 0, poison: 0, scarletRot: 0, sleep: 0, madness: 0, death: 0 } };
    }
    if (mutation === "identity") preset.selectedBuild.weaponId += 123456;
    if (mutation === "upgrade") { preset.selectedBuild.isSomber = true; preset.selectedBuild.upgrade = 25; }
    await importPreset(page, preset);
    const original = await page.evaluate(id => localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${id}`), preset.id);
    await page.getByRole("button", { name: "Load", exact: true }).click();
    if (mutation === "identity" || mutation === "upgrade") {
      await expect(page.locator('.error-strip[role="alert"]')).toContainText(/contradict|cannot be verified|upgrade/i);
      await expect(page.locator(".result-row-full")).toHaveCount(0);
    } else {
      await expect(page.getByText(/saved results verified on current data/)).toBeVisible();
      const result = await page.evaluate(async () => {
        const { useDesktopStore } = await import("/src/lib/state.ts");
        return useDesktopStore.getState().selected;
      });
      expect(result!.ar.total).not.toBe(999999);
      expect(result!.aowFirstHitDamage).not.toBe(999999);
      expect(result!.aowRoute?.routeId).not.toBe("forged");
    }
    expect(await page.evaluate(id => localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${id}`), preset.id)).toBe(original);
  });
}

test("contains an unexpected render failure and reloads without deleting saved builds", async ({ page }) => {
  const preset = await saved(page, false);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().patchRequest({ className: "Unexpected broken state" });
  });
  await expect(page.getByRole("heading", { name: "The workspace could not be displayed" })).toBeVisible();
  await page.getByRole("button", { name: "Reload workspace" }).click();
  await page.getByText("Snapshot loaded", { exact: true }).waitFor();
  expect(await page.evaluate(id => localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${id}`), preset.id)).not.toBeNull();
});

for (const stage of ["setup", "commit", "earlier error"] as const) {
  test(`contains storage denial at migration ${stage}`, async ({ page }) => {
    const preset = await saved(page);
    preset.dataVersion = "vanilla:4:old:old";
    await page.evaluate(preset => localStorage.setItem(`tarnisheds-arsenal.savedBuild.v2.${preset.id}`, JSON.stringify(preset)), preset);
    await page.reload();
    await page.getByText("Snapshot loaded", { exact: true }).waitFor();
    await page.evaluate(async stage => {
      const { api } = await import("/src/lib/api.ts");
      const read = Storage.prototype.getItem;
      let denied = stage === "setup";
      Storage.prototype.getItem = function (key) {
        if (denied && key.startsWith("tarnisheds-arsenal.savedBuild.v2.")) throw new Error("saved storage denied");
        return read.call(this, key);
      };
      const solve = api.solveBuild;
      api.solveBuild = async (...args) => {
        const row = await solve(...args);
        denied = true;
        if (stage === "earlier error") throw new Error("native verification failed");
        return row;
      };
      Object.assign(window, { restoreSavedReads: () => { Storage.prototype.getItem = read; } });
    }, stage);
    await page.getByRole("button", { name: "Migrate data", exact: true }).click();
    await expect(page.locator('.error-strip[role="alert"]')).toContainText(stage === "earlier error" ? "native verification failed" : "saved storage denied");
    await page.evaluate(async () => {
      (window as any).restoreSavedReads();
      const { useDesktopStore } = await import("/src/lib/state.ts");
      useDesktopStore.getState().patchRequest({ dex: 35 });
    });
    await expect(page.getByRole("button", { name: "Migrate data", exact: true })).toBeEnabled();
    expect(await page.evaluate(id => JSON.parse(localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${id}`)!).dataVersion, preset.id)).toBe("vanilla:4:old:old");
  });
}
