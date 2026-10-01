import { expect, test, type Page } from "@playwright/test";
import { closeEditors, openEditor } from "./editors";

async function setup(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const { clearAnalysisCaches } = await import("/src/lib/analysis-cache.ts");
    const originalProfile = api.weaponProfile;
    const probe: any = { searches: [], pending: [], cancels: [] };
    api.startSearch = async (request) => {
      probe.searches.push(request);
      return { jobId: `selection-${probe.searches.length}` };
    };
    api.searchStatus = async () => ({ progress: null, finished: null });
    api.cancelSearch = async (jobId) => { probe.cancels.push(jobId); return true; };
    probe.holdProfiles = () => {
      clearAnalysisCaches();
      api.weaponProfile = (profileId, weaponName, affinity) => new Promise((resolve, reject) => {
        probe.pending.push({ key: [profileId, weaponName, affinity], resolve, reject });
      });
    };
    probe.resolve = async (key, overrides = {}, latestOnly = false) => {
      const matches = probe.pending.filter(item => JSON.stringify(item.key) === JSON.stringify(key));
      const items = latestOnly ? matches.slice(-1) : matches;
      probe.pending = probe.pending.filter(item => !items.includes(item));
      const profile = { ...await originalProfile(...key), ...overrides };
      items.forEach(item => item.resolve(profile));
    };
    probe.reject = (key) => {
      const items = probe.pending.filter(item => JSON.stringify(item.key) === JSON.stringify(key));
      probe.pending = probe.pending.filter(item => !items.includes(item));
      items.forEach(item => item.reject(new Error("Metadata unavailable")));
    };
    probe.patch = (patch) => {
      if (patch.profileId && patch.profileId !== useDesktopStore.getState().request.profileId) {
        // Hold the same selected weapon across profiles to exercise hook identity,
        // independently of the normal profile switch resetting the whole query.
        useDesktopStore.setState(state => ({ request: { ...state.request, ...patch } }));
      } else useDesktopStore.getState().patchRequest(patch);
    };
    Object.assign(window, { selectionProbe: probe });
  });
}

async function loadoutField(page: Page, name: "Weapon" | "AoW") {
  return (await openEditor(page, "Loadout")).getByRole("combobox", { name, exact: true });
}

async function patch(page: Page, patch: Record<string, unknown>) {
  await page.evaluate(value => (window as any).selectionProbe.patch(value), patch);
}

async function pending(page: Page, key: Array<string | null>) {
  await expect.poll(() => page.evaluate(key => (window as any).selectionProbe.pending.some(item =>
    JSON.stringify(item.key) === JSON.stringify(key)), key)).toBe(true);
}

async function resolve(page: Page, key: Array<string | null>, overrides = {}) {
  await pending(page, key);
  await page.evaluate(({ key, overrides }) => (window as any).selectionProbe.resolve(key, overrides), { key, overrides });
}

test("an exact typed weapon commits before immediate Search dispatch", async ({ page }) => {
  await setup(page);
  await (await loadoutField(page, "Weapon")).fill("Zweihander");
  // The same browser turn models blur immediately followed by submission, without
  // an automation wait allowing a delayed commit to hide the ordering bug.
  await page.evaluate(() => {
    document.querySelector<HTMLInputElement>('input[aria-label="Weapon"]')!.blur();
    document.querySelector<HTMLButtonElement>(".search-button")!.click();
  });
  await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.searches[0]?.weaponName)).toBe("Zweihander");
  await page.waitForTimeout(180);
  expect(await page.evaluate(() => (window as any).selectionProbe.cancels)).toEqual([]);
});

for (const interruption of ["cancel", "edit", "navigate"] as const) {
  test(`loadout preparation cannot dispatch after ${interruption}`, async ({ page }) => {
    await setup(page);
    await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
    await patch(page, { weaponName: "Zweihander" });
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("Checking loadout…", { exact: true })).toBeVisible();
    if (interruption === "cancel") await page.getByRole("button", { name: "Cancel Search", exact: true }).click();
    else if (interruption === "edit") {
      await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("21");
      await page.getByRole("spinbutton", { name: "STR", exact: true }).press("Enter");
    } else await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      useDesktopStore.getState().setWorkspace("compare");
    });
    await expect(page.getByText("Checking loadout…", { exact: true })).toHaveCount(0);
    await resolve(page, ["vanilla", "Zweihander", null], {
      nativeSkillName: "Stamp (Upward Cut)", compatibleAows: ["Stamp (Upward Cut)"],
    });
    await page.waitForTimeout(180);
    expect(await page.evaluate(() => (window as any).selectionProbe.searches)).toEqual([]);
    expect(await page.evaluate(() => (window as any).selectionProbe.cancels)).toEqual([]);
  });
}

test("delayed skill defaults cannot invalidate an immediately submitted search", async ({ page }) => {
  await setup(page);
  await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
  await (await loadoutField(page, "Weapon")).fill("Zweihander");
  await page.evaluate(() => {
    document.querySelector<HTMLInputElement>('input[aria-label="Weapon"]')!.blur();
    document.querySelector<HTMLButtonElement>(".search-button")!.click();
  });
  await resolve(page, ["vanilla", "Zweihander", null], {
    nativeSkillName: "Stamp (Upward Cut)", compatibleAows: ["Stamp (Upward Cut)"],
  });
  await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.searches[0]?.weaponName)).toBe("Zweihander");
  expect(await page.evaluate(() => (window as any).selectionProbe.searches[0]?.aowName)).toBe("Stamp (Upward Cut)");
  await page.waitForTimeout(180);
  expect(await page.evaluate(() => (window as any).selectionProbe.cancels)).toEqual([]);
});

for (const interruption of ["cancel", "STR edit", "lookup failure"] as const) {
  test(`an undispatched ${interruption} preserves the pending manual native default`, async ({ page }) => {
    await setup(page);
    await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
    const weapon = await loadoutField(page, "Weapon");
    await weapon.fill("Zweihander");
    await weapon.press("Tab");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("Checking loadout…", { exact: true })).toBeVisible();
    if (interruption === "cancel") await page.getByRole("button", { name: "Cancel Search", exact: true }).click();
    else if (interruption === "STR edit") await patch(page, { strStat: 21 });
    else {
      await page.evaluate(() => (window as any).selectionProbe.reject(["vanilla", "Zweihander", null]));
      await page.getByRole("button", { name: "Retry weapon profile", exact: true }).click();
      await openEditor(page, "Loadout");
      await page.getByRole("button", { name: "Retry AoW skills", exact: true }).click();
    }
    await expect(page.getByText("Checking loadout…", { exact: true })).toHaveCount(0);
    await resolve(page, ["vanilla", "Zweihander", null], {
      nativeSkillName: "Stamp (Upward Cut)", compatibleAows: ["Stamp (Upward Cut)"],
    });
    await expect(await loadoutField(page, "AoW")).toHaveValue("Stamp (Upward Cut)");
    expect(await page.evaluate(() => (window as any).selectionProbe.searches)).toEqual([]);
  });
}

test("starter Automatic is not overwritten by late native-skill metadata", async ({ page }) => {
  await setup(page);
  await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
  await (await loadoutField(page, "Weapon")).fill("Uchigatana");
  await (await loadoutField(page, "Weapon")).press("Tab");
  await pending(page, ["vanilla", "Uchigatana", null]);
  await page.getByRole("button", { name: "Try Uchigatana +3 example", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.searches.length)).toBe(1);
  await resolve(page, ["vanilla", "Uchigatana", "Standard"], {
    nativeSkillName: "Unsheathe", compatibleAows: ["Unsheathe", "Seppuku"], affinities: ["Standard", "Blood", "Occult"],
  });
  await expect(await loadoutField(page, "AoW")).toHaveValue("Automatic (best legal skill)");
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  expect(await page.evaluate(() => (window as any).selectionProbe.searches[0].aowName)).toBeNull();
  expect(await page.evaluate(() => (window as any).selectionProbe.cancels)).toEqual([]);
  await expect(page.getByRole("button", { name: "Cancel Search", exact: true })).toBeVisible();
});

for (const editBeforeMetadata of [true, false]) {
  test(`manual native defaults survive a STR edit ${editBeforeMetadata ? "before" : "after"} metadata`, async ({ page }) => {
    await setup(page);
    await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
    const weapon = await loadoutField(page, "Weapon");
    await weapon.fill("Zweihander");
    await weapon.press("Tab");
    await pending(page, ["vanilla", "Zweihander", null]);
    if (editBeforeMetadata) {
      await patch(page, { strStat: 21 });
      await page.getByRole("button", { name: "Search", exact: true }).click();
    }
    await resolve(page, ["vanilla", "Zweihander", null], {
      nativeSkillName: "Stamp (Upward Cut)", compatibleAows: ["Stamp (Upward Cut)"],
    });
    await expect(await loadoutField(page, "AoW")).toHaveValue("Stamp (Upward Cut)");
    if (!editBeforeMetadata) {
      await patch(page, { strStat: 21 });
      await page.getByRole("button", { name: "Search", exact: true }).click();
    }
    await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.searches[0]?.aowName)).toBe("Stamp (Upward Cut)");
    expect(await page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().request.strStat)).toBe(21);
  });
}

for (const submitted of [false, true]) for (const replacement of ["preset", "explicit Automatic"] as const) {
  test(`same-weapon ${replacement} retires pending manual defaults${submitted ? " after Search" : ""}`, async ({ page }) => {
    await setup(page);
    await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
    const weapon = await loadoutField(page, "Weapon");
    await weapon.fill("Uchigatana");
    await weapon.press("Tab");
    await pending(page, ["vanilla", "Uchigatana", null]);
    if (submitted) {
      await page.getByRole("button", { name: "Search", exact: true }).click();
      await expect(page.getByText("Checking loadout…", { exact: true })).toBeVisible();
    }
    await page.evaluate(async replacement => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const state = useDesktopStore.getState();
      if (replacement === "explicit Automatic") state.patchRequest({ aowName: null });
      else {
        const manifest = state.catalog!.dataManifest;
        await state.loadBuildPreset({ version: 2, id: "same-weapon", name: "Same weapon Automatic", profileId: "vanilla",
          request: { ...state.request, aowName: null }, selectedBuild: null, compareTarget: null, compareBench: [],
          createdAt: "2026-09-29", updatedAt: "2026-09-29",
          dataVersion: `${manifest.profile.id}:${manifest.schemaVersion}:${manifest.datasetVersion}:${manifest.modelVersion}` });
      }
    }, replacement);
    if (submitted) await expect(page.getByText("Checking loadout…", { exact: true })).toHaveCount(0);
    await resolve(page, ["vanilla", "Uchigatana", null], {
      nativeSkillName: "Unsheathe", compatibleAows: ["Unsheathe", "Seppuku"],
    });
    await expect(await loadoutField(page, "AoW")).toHaveValue("Automatic (best legal skill)");
    expect(await page.evaluate(() => (window as any).selectionProbe.searches)).toEqual([]);
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.searches.length)).toBe(1);
    expect(await page.evaluate(() => (window as any).selectionProbe.searches[0].aowName)).toBeNull();
  });
}

test("Escape then Enter never accepts a hidden option", async ({ page }) => {
  await setup(page);
  await patch(page, { weaponName: "Zweihander" });
  const input = await loadoutField(page, "Weapon");
  await expect(input).toHaveValue("Zweihander");
  await input.focus();
  await input.press("Escape");
  await input.press("Enter");
  await expect(input).toHaveValue("Zweihander");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.searches[0]?.weaponName)).toBe("Zweihander");
});

test("option click, Tab, unmatched text, and rapid refocus preserve committed selections", async ({ page }) => {
  await setup(page);
  const input = await loadoutField(page, "Weapon");
  await input.click();
  await page.getByRole("option", { name: "Zweihander", exact: true }).click();
  await expect(input).toHaveValue("Zweihander");
  await input.fill("Uchigatana");
  await input.press("Tab");
  await expect(input).not.toBeFocused();
  await expect(input).toHaveValue("Uchigatana");
  await input.fill("No such weapon");
  await input.press("Tab");
  await expect(input).toHaveValue("Uchigatana");
  await input.fill("Zweihander");
  await page.evaluate(() => {
    const input = document.querySelector<HTMLInputElement>('input[aria-label="Weapon"]')!;
    input.blur(); input.focus();
  });
  await input.press("Escape");
  await expect(input).toHaveValue("Zweihander");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.searches[0]?.weaponName)).toBe("Zweihander");
});

test("keyboard navigation scrolls the active option into view", async ({ page }) => {
  await setup(page);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const state = useDesktopStore.getState();
    useDesktopStore.setState({ catalog: { ...state.catalog!, weaponNames: Array.from({ length: 50 }, (_, i) => `Weapon ${i}`) } });
  });
  const input = await loadoutField(page, "Weapon");
  await input.focus();
  for (let i = 0; i < 35; i++) await input.press("ArrowDown");
  expect(await input.evaluate(input => {
    const option = document.getElementById(input.getAttribute("aria-activedescendant")!)!;
    const list = option.parentElement!;
    const item = option.getBoundingClientRect();
    const menu = list.getBoundingClientRect();
    return item.top >= menu.top && item.bottom <= menu.bottom;
  })).toBe(true);
});

for (const next of [
  { weaponName: "Zweihander", affinity: null, profileId: "vanilla" },
  { weaponName: "Uchigatana", affinity: "Keen", profileId: "vanilla" },
  { weaponName: "Uchigatana", affinity: null, profileId: "convergence" },
]) {
  test(`metadata is hidden until the matching key resolves: ${JSON.stringify(next)}`, async ({ page }) => {
    await setup(page);
    await patch(page, { weaponName: "Uchigatana" });
    await expect(page.locator(".requirements-strip")).toContainText("STR 11");
    await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
    await patch(page, next);
    const key = [next.profileId, next.weaponName, next.affinity];
    await pending(page, key);
    await expect(page.locator(".requirements-strip")).toHaveCount(0);
    await resolve(page, key, { requirements: { strStat: 20, dex: 0, intStat: 0, fai: 0, arc: 0 } });
    await expect(page.locator(".requirements-strip")).toContainText("STR 20");
  });
}

test("failed metadata has a retry path and old completions cannot replace current data", async ({ page }) => {
  await setup(page);
  await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
  await patch(page, { weaponName: "Uchigatana" });
  await pending(page, ["vanilla", "Uchigatana", null]);
  await patch(page, { weaponName: "Zweihander" });
  await pending(page, ["vanilla", "Zweihander", null]);
  await page.evaluate(() => (window as any).selectionProbe.reject(["vanilla", "Zweihander", null]));
  const retry = page.getByRole("button", { name: "Retry weapon profile", exact: true });
  await expect(retry).toBeVisible();
  await retry.click();
  await resolve(page, ["vanilla", "Zweihander", null]);
  await expect(page.locator(".requirements-strip")).toContainText("STR 19");
  await resolve(page, ["vanilla", "Uchigatana", null]);
  await expect(page.locator(".requirements-strip")).toContainText("STR 19");
});

for (const unrelatedError of [null, "Search failed independently"]) {
  test(`metadata retries leave global errors unchanged: ${unrelatedError ?? "no global error"}`, async ({ page }) => {
    await setup(page);
    await page.evaluate(async error => {
      (window as any).selectionProbe.holdProfiles();
      const { useDesktopStore } = await import("/src/lib/state.ts");
      useDesktopStore.getState().setError(error);
    }, unrelatedError);
    await patch(page, { weaponName: "Zweihander" });
    await pending(page, ["vanilla", "Zweihander", null]);
    await page.evaluate(() => (window as any).selectionProbe.reject(["vanilla", "Zweihander", null]));
    await expect(page.getByText("Weapon profile unavailable: Metadata unavailable", { exact: true })).toBeVisible();
    await openEditor(page, "Loadout");
    await expect(page.getByText("Skills unavailable: Metadata unavailable", { exact: true })).toBeVisible();
    await closeEditors(page);
    await page.getByRole("button", { name: "Retry weapon profile", exact: true }).click();
    await openEditor(page, "Loadout");
    await page.getByRole("button", { name: "Retry AoW skills", exact: true }).click();
    await resolve(page, ["vanilla", "Zweihander", null]);
    await expect(page.locator(".requirements-strip")).toContainText("STR 19");
    await expect(await loadoutField(page, "AoW")).toBeEnabled();
    await expect(page.getByText(/unavailable: Metadata unavailable/)).toHaveCount(0);
    if (unrelatedError) await expect(page.locator(".error-strip")).toHaveText(unrelatedError);
    else await expect(page.locator(".error-strip")).toHaveCount(0);
  });
}

test("A to B to A accepts only the current lookup even when older replies arrive last", async ({ page }) => {
  await setup(page);
  await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
  await patch(page, { weaponName: "Uchigatana" });
  await pending(page, ["vanilla", "Uchigatana", null]);
  await patch(page, { weaponName: "Zweihander" });
  await pending(page, ["vanilla", "Zweihander", null]);
  await patch(page, { weaponName: "Uchigatana" });
  await expect.poll(() => page.evaluate(() => (window as any).selectionProbe.pending.filter(item => item.key[1] === "Uchigatana").length)).toBeGreaterThan(1);
  await page.evaluate(() => (window as any).selectionProbe.resolve(["vanilla", "Uchigatana", null], {
    requirements: { strStat: 17, dex: 0, intStat: 0, fai: 0, arc: 0 },
  }, true));
  await expect(page.locator(".requirements-strip")).toContainText("STR 17");
  await resolve(page, ["vanilla", "Zweihander", null]);
  await resolve(page, ["vanilla", "Uchigatana", null]);
  await expect(page.locator(".requirements-strip")).toContainText("STR 17");
});

test("forced handling meets STR requirements without the user toggle", async ({ page }) => {
  await setup(page);
  await page.evaluate(() => (window as any).selectionProbe.holdProfiles());
  await patch(page, { weaponName: "Zweihander", strStat: 14, twoHanding: false });
  await resolve(page, ["vanilla", "Zweihander", null], {
    forcesTwoHanding: true, disablesTwoHandBonus: false,
    requirements: { strStat: 20, dex: 0, intStat: 0, fai: 0, arc: 0 },
  });
  await expect(page.locator(".requirements-strip")).toContainText("STR 20");
  await expect(page.locator(".requirements-strip")).not.toHaveClass(/missing/);
});
