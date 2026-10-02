import { expect, test, type Page } from "@playwright/test";
import { openCompare, openEditor } from "./editors";

async function prepare(page: Page, pins = false) {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
  return page.evaluate(async pins => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const { api } = await import("/src/lib/api.ts");
    const { clearAnalysisCaches } = await import("/src/lib/analysis-cache.ts");
    clearAnalysisCaches();
    const rows = useDesktopStore.getState().rows;
    if (pins) rows.slice(1, 3).forEach(row => useDesktopStore.getState().toggleCompareBench(row));
    api.solveBuild = async (_base, name, affinity) => rows.find(row => row.weaponName === name && row.affinity === affinity) ?? null;
    const probe = { start: performance.now(), calls: [] as any[], cancelled: [] as string[] };
    api.buildUpgradeSeries = (_base, row, _cap, signal) => new Promise((resolve, reject) => {
      probe.calls.push({ name: row.weaponName, resolve: () => resolve([{ upgrade: 0, metric: row.ar.total }]),
        reject: () => reject(new Error("controlled chart failure")) });
      signal?.addEventListener("abort", () => { probe.cancelled.push(row.weaponName); reject(new Error("cancelled")); }, { once: true });
    });
    Object.assign(window, { comparisonProbe: probe });
    return rows.map(row => row.weaponName);
  }, pins);
}

test("publishes multiple verified pins and scaling before any optional chart finishes", async ({ page }, testInfo) => {
  const names = await prepare(page, true);
  await openCompare(page);
  await expect(page.getByRole("group", { name: "Pinned #1", exact: true })).toContainText(names[1]);
  await expect(page.getByRole("table", { name: "Primary deltas versus baseline" })).toBeVisible();
  await expect(page.getByRole("group", { name: "Selected baseline", exact: true }).getByRole("listitem", { name: "Strength scaling: C", exact: true })).toBeVisible();
  const firstMs = await page.evaluate(() => performance.now() - (window as any).comparisonProbe.start);
  await expect(page.getByText("Upgrade chart loading…", { exact: true })).toHaveCount(3);
  await page.evaluate(() => (window as any).comparisonProbe.calls[0].resolve());
  await expect(page.getByText("Upgrade chart loading…", { exact: true })).toHaveCount(2);
  await page.evaluate(async () => {
    await new Promise(resolve => setTimeout(resolve, 600));
    (window as any).comparisonProbe.calls.forEach((call: any) => call.resolve());
  });
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  const allMs = await page.evaluate(() => performance.now() - (window as any).comparisonProbe.start);
  await testInfo.attach("controlled-comparison-timing", { body: JSON.stringify({ firstValidComparisonMs: firstMs, allChartsReadyMs: allMs, syntheticHeldResponses: true }), contentType: "application/json" });
  console.log(JSON.stringify({ firstValidComparisonMs: firstMs, allChartsReadyMs: allMs, syntheticHeldResponses: true }));
  expect(firstMs).toBeLessThan(allMs);
});

test("a failed optional chart preserves verified deltas and other charts", async ({ page }) => {
  const names = await prepare(page);
  await openCompare(page);
  await expect.poll(() => page.evaluate(() => (window as any).comparisonProbe.calls.length)).toBe(4);
  await page.evaluate(() => (window as any).comparisonProbe.calls.forEach((call: any, i: number) => i === 1 ? call.reject() : call.resolve()));
  await expect(page.getByText("Upgrade chart unavailable: controlled chart failure", { exact: true })).toBeVisible();
  await expect(page.getByRole("group", { name: "Top #2", exact: true })).toContainText(names[1]);
  await expect(page.getByRole("table", { name: "Primary deltas versus baseline" }).getByRole("row")).toHaveCount(4);
  expect(await page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().compareTarget?.weaponName)).toBe(names[1]);
  await expect(page.locator(".matrix-row").filter({ hasText: "Selected" }).getByRole("gridcell").first()).not.toHaveText("—");
});

test("request changes cancel optional charts and cannot republish old comparison rows", async ({ page }) => {
  await prepare(page, true);
  await openCompare(page);
  await expect(page.getByRole("group", { name: "Pinned #1", exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().patchRequest({ twoHanding: true });
    (window as any).comparisonProbe.calls.forEach((call: any) => call.resolve());
  });
  await expect(page.getByText("Update Rankings before comparing", { exact: true })).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as any).comparisonProbe.cancelled.length)).toBeGreaterThan(0);
  expect(await page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().compareTarget)).toBeNull();
});

test("stamina desirability reverses cost direction without changing arithmetic signs", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const state = useDesktopStore.getState();
    state.setRows(state.rows.map((row, i) => ({ ...row, ar: { ...row.ar, total: 100 + [0, 5, -5, 0][i] },
      aowRoute: { routeId: "cost", routeLabel: "Cost", routePriority: 0, buffActivationActionId: null,
        actions: [], firstHitDamage: 0, totalDamage: row.ar, totalPoiseDamage: 0,
        totalStaminaCost: [10, 15, 5, 10][i],
        totalStatusBuildup: { bleed: 0, frost: 0, poison: 0, scarletRot: 0, sleep: 0, madness: 0, death: 0 } } })));
  });
  await openCompare(page);
  await page.getByText("Full metric breakdown", { exact: true }).click();
  const rows = page.getByRole("table", { name: "All candidate deltas versus baseline" }).getByRole("row");
  for (const [index, text, staminaClass, damageClass] of [[1, "+5.0", "negative", "positive"], [2, "-5.0", "positive", "negative"], [3, "0.0", "", ""]] as const) {
    await expect(rows.nth(index).getByRole("cell").last()).toHaveText(text);
    await expect(rows.nth(index).getByRole("cell").last()).toHaveAttribute("class", staminaClass);
    await expect(rows.nth(index).getByRole("cell").first()).toHaveAttribute("class", damageClass);
  }
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const state = useDesktopStore.getState();
    state.setRows(state.rows.map((row, i) => i === 1 ? { ...row, aowRoute: null } : row));
  });
  await page.getByText("Full metric breakdown", { exact: true }).click();
  await expect(rows.nth(1).getByRole("cell").last()).toHaveText("Unavailable");
  await expect(rows.nth(1).getByRole("cell").last()).not.toHaveAttribute("class", /positive|negative/);
});

test("only a verified non-upgradeable pin selects explicit +0 under an exact budget", async ({ page }) => {
  await prepare(page);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const { api } = await import("/src/lib/api.ts");
    const state = useDesktopStore.getState();
    const [selected, first, second] = state.rows;
    state.patchRequest({ exactUpgrade: true, standardMaxUpgrade: 20 });
    state.setRows([selected, { ...first, upgrade: 0 }, second]);
    state.rows.slice(1, 3).forEach(row => useDesktopStore.getState().toggleCompareBench(row));
    const profile = api.weaponProfile;
    api.weaponProfile = async (...args) => ({ ...await profile(...args), maxUpgrade: args[2] === first.affinity ? 0 : 25 });
    const calls: any[] = [];
    api.solveBuild = async (base, name, affinity) => {
      calls.push({ affinity, standardMaxUpgrade: base.standardMaxUpgrade, exactUpgrade: base.exactUpgrade });
      const row = useDesktopStore.getState().rows.find(row => row.weaponName === name && row.affinity === affinity)!;
      return { ...row, upgrade: base.standardMaxUpgrade };
    };
    Object.assign(window, { exactPinCalls: calls });
  });
  await openCompare(page);
  await expect.poll(() => page.evaluate(() => (window as any).exactPinCalls)).toEqual([
    { affinity: "Occult", standardMaxUpgrade: 0, exactUpgrade: true },
    { affinity: "Keen", standardMaxUpgrade: 20, exactUpgrade: true },
  ]);
  await expect(page.getByRole("group", { name: "Pinned #1 (+0 only)", exact: true })).toContainText("+0");
});

test("manual comparison filters preserve pins and unpinning invalidates the Paths target", async ({ page }) => {
  await prepare(page, true);
  await openCompare(page);
  await expect(page.getByRole("group", { name: "Pinned #1", exact: true })).toBeVisible();
  await openEditor(page, "Comparison filters");
  await page.getByRole("checkbox", { name: "Smithing", exact: true }).uncheck();
  await expect(page.getByRole("button", { name: "Use pinned targets", exact: true })).toBeVisible();
  expect(await page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().compareBench.length)).toBe(2);
  await page.getByRole("button", { name: "Use pinned targets", exact: true }).click();
  await expect(page.getByRole("group", { name: "Pinned #1", exact: true })).toBeVisible();
  await page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true }).click();
  await page.locator(".result-row-full").nth(1).getByRole("button", { name: /^Unpin / }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
  await expect(page.locator(".path-lane").nth(1)).toContainText("No compare lane selected.");
  expect(await page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().compareTarget)).toBeNull();
});

test("an infeasible first pin does not hide the next verified Paths target", async ({ page }) => {
  await prepare(page, true);
  const expected = await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const { api } = await import("/src/lib/api.ts");
    const [, valid] = useDesktopStore.getState().compareBench;
    api.solveBuild = async (_base, _name, affinity) => affinity === valid.affinity ? valid : null;
    return valid;
  });
  await openCompare(page);
  await expect(page.getByRole("group", { name: "Pinned #1", exact: true })).toContainText("No compatible target");
  await expect(page.getByRole("group", { name: "Pinned #2", exact: true })).toContainText(expected.affinity);
  expect(await page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().compareTarget)).toEqual(expected);
  await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
  await expect(page.locator(".path-lane").nth(1)).toContainText(expected.affinity);
});

for (const conflict of ["somber", "excluded-type", "weapon-and-skill", "locks"] as const) {
  test(`starter removes conflicting ${conflict} discovery inputs while preserving the character`, async ({ page }) => {
    await page.goto("/");
    await page.getByText("Snapshot loaded", { exact: true }).waitFor();
    const before = await page.evaluate(async conflict => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const { api } = await import("/src/lib/api.ts");
      const state = useDesktopStore.getState();
      const profile = api.weaponProfile;
      api.weaponProfile = async (...args) => ({ ...await profile(...args), affinities: ["Standard", "Blood", "Occult", "Keen"] });
      const patch = conflict === "somber" ? { somberFilter: "somber_only" }
        : conflict === "excluded-type" ? { filters: { version: 1, entries: [{ dimension: "weapon_type", id: state.catalog!.filterDimensions.find(d => d.id === "weapon_type")!.options.find(o => o.label === "Katana")!.id, mode: "exclude" }] } }
          : conflict === "weapon-and-skill" ? { weaponName: "Claymore", aowName: "Lion's Claw", weaponTypeKey: "Greatsword" }
            : { lockStr: 10, lockDex: 10 };
      state.patchRequest(patch as any);
      if (conflict === "locks") state.setLockedStatMode(true);
      const start = api.startSearch;
      api.startSearch = async request => {
        Object.assign(window, { starterRequest: request });
        return start(request);
      };
      return useDesktopStore.getState().request;
    }, conflict);
    await page.getByRole("button", { name: "Try Uchigatana +3 example", exact: true }).click();
    await expect.poll(() => page.evaluate(() => Boolean((window as any).starterRequest))).toBe(true);
    const request = await page.evaluate(() => (window as any).starterRequest);
    expect(request).toMatchObject({ weaponTypeKey: null, weaponName: "Uchigatana", affinity: "Standard", aowName: null,
      somberFilter: "all", filters: { version: 1, entries: [] }, standardMaxUpgrade: 3, exactUpgrade: true, objective: "max_ar" });
    for (const field of ["className", "vig", "mnd", "end", "minStr", "minDex", "minInt", "minFai", "minArc", "lockStr", "lockDex", "lockInt", "lockFai", "lockArc"] as const) expect(request[field]).toBe(before[field]);
    await expect(page.getByText(/Character stats, floors, locks, and world settings are retained/)).toBeVisible();
    if (conflict === "locks") await expect(page.getByText(/No legal Uchigatana \+3 build fits your retained character constraints/)).toBeVisible();
  });
}
