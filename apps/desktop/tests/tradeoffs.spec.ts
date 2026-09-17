import { expect, test } from "@playwright/test";

async function openCompare(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
}

test("tradeoff views select exact threshold choices and persist an exact apply", async ({ page }) => {
  await openCompare(page);
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const original = api.arBleedFrontier;
    (window as any).frontierCalls = 0;
    api.arBleedFrontier = async (...args) => {
      (window as any).frontierCalls++;
      const points = await original(...args);
      // Rounded AR looks eligible at 1%; the exact threshold says otherwise.
      points[1].minimumArLossBps = 101;
      return points;
    };
  });
  const section = page.getByRole("region", { name: "AR / Bleed tradeoffs" });
  await section.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(section.getByRole("status")).toContainText("4 exact trade-off points");
  const shortlist = section.locator(".tradeoff-shortlist");
  await expect(shortlist.locator("tbody tr")).toHaveCount(3);
  await expect(shortlist.getByRole("button", { name: /^Max AR/ })).toContainText("Within 1%");
  const input = section.getByRole("spinbutton", { name: "Max AR sacrifice (%)" });
  await expect(section.locator("#tradeoff-sacrifice-help")).toContainText("this fixed loadout's maximum AR");
  await input.fill("1");
  await expect(section.locator(".tradeoff-inspection")).toContainText("Gain 0.0 bleed buildup");
  await input.fill("1.01");
  await expect(section.locator(".tradeoff-inspection")).toContainText("Gain 5.0 bleed buildup");
  await input.fill("1.001");
  await expect(section.getByRole("alert")).toContainText("steps of 0.01%");
  await section.getByText("Explore all tradeoffs (4)", { exact: true }).click();
  await expect(section.locator(".tradeoff-all tbody tr")).toHaveCount(4);
  const point = section.getByRole("button", { name: /^Point 4:/ });
  await point.press("Enter");
  await expect(point).toHaveAttribute("aria-pressed", "true");
  await expect(section.locator(".tradeoff-inspection")).toContainText("Gain 15.0 bleed buildup");
  await expect(section.locator(".tradeoff-inspection")).toContainText("VIG 12 / MND 11 / END 13");
  const chosenPointRow = section.locator(".tradeoff-all tbody tr.selected");
  await expect(chosenPointRow).toHaveCount(1);
  const chosenPointAr = await chosenPointRow.locator("td").nth(0).innerText();
  const chosenPointBleed = await chosenPointRow.locator("td").nth(1).innerText();
  expect(chosenPointAr).toBe("665.0");
  expect(chosenPointBleed).toBe("99.0");
  expect(await page.evaluate(() => (window as any).frontierCalls)).toBe(1);
  await expect(page.locator(".selected-build")).toContainText("Blood / Seppuku / +25");
  await page.evaluate(async ({ chosenAr, chosenBleed }) => {
    const [{ api }, { useDesktopStore }] = await Promise.all([
      import("/src/lib/api.ts"),
      import("/src/lib/state.ts"),
    ]);
    const selected = useDesktopStore.getState().selected;
    if (!selected) throw new Error("Exact apply fixture requires a selected build.");
    let appliedResult = selected;
    api.startSearch = async request => {
      (window as any).appliedTradeoffRequest = request;
      // Browser preview rows ignore stat locks; this controlled reply exercises the apply/persist path with a lock-compliant result.
      appliedResult = {
        ...selected,
        weaponName: request.weaponName ?? selected.weaponName,
        affinity: request.affinity ?? selected.affinity,
        aowName: request.aowName ?? selected.aowName,
        upgrade: selected.isSomber ? request.somberMaxUpgrade : request.standardMaxUpgrade,
        stats: {
          ...selected.stats,
          strStat: request.lockStr ?? selected.stats.strStat,
          dex: request.lockDex ?? selected.stats.dex,
          intStat: request.lockInt ?? selected.stats.intStat,
          fai: request.lockFai ?? selected.stats.fai,
          arc: request.lockArc ?? selected.stats.arc,
        },
        ar: { ...selected.ar, physical: chosenAr, total: chosenAr },
        bleedBuildup: chosenBleed,
        score: chosenAr,
      };
      (window as any).appliedTradeoffResult = appliedResult;
      return { jobId: "browser-tradeoff-apply" };
    };
    api.searchStatus = async jobId => ({
      progress: null,
      finished: { jobId, cancelled: false, rows: [appliedResult], error: null },
    });
  }, { chosenAr: Number(chosenPointAr), chosenBleed: Number(chosenPointBleed) });
  await section.getByRole("button", { name: "Use exact allocation", exact: true }).click();
  await expect(page.locator(".result-row-full")).toHaveCount(1);
  const applied = await page.evaluate(() => (window as any).appliedTradeoffRequest);
  expect(applied).toMatchObject({
    weaponName: "Uchigatana", affinity: "Blood", aowName: "Seppuku",
    objective: "max_ar", exactUpgrade: true, standardMaxUpgrade: 25,
    lockStr: 13, lockDex: 19, lockInt: 9, lockFai: 8, lockArc: 63,
  });
  const appliedRow = page.locator(".result-row-full").first();
  await expect(appliedRow.locator(".weapon-cell strong")).toHaveText("Uchigatana");
  await expect(appliedRow.locator(".setup-cell strong")).toHaveText("Blood");
  await expect(appliedRow.locator(".setup-cell > small")).toHaveText("Seppuku");
  await expect(appliedRow.getByRole("gridcell").nth(3)).toHaveText("+25");
  await expect(appliedRow.locator(".row-combat-stats")).toHaveText("STR 13 / DEX 19 / INT 9 / FAI 8 / ARC 63");
  await expect(appliedRow.locator(".ar-status-cell strong")).toHaveText("665");
  await expect(page.locator('[aria-label="Bleed buildup: 99"]')).toBeVisible();
  const returnedResult = await page.evaluate(() => (window as any).appliedTradeoffResult);
  expect(returnedResult).toMatchObject({
    weaponName: "Uchigatana", affinity: "Blood", aowName: "Seppuku", upgrade: 25,
    stats: { strStat: 13, dex: 19, intStat: 9, fai: 8, arc: 63 },
    ar: { total: 665 }, bleedBuildup: 99,
  });

  const presetName = `Tradeoff exact apply ${Date.now()}`;
  await page.getByRole("textbox", { name: "Name", exact: true }).fill(presetName);
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByText(`Saved ${presetName}.`, { exact: true }).waitFor();
  await page.reload();
  await page.getByRole("combobox", { name: "Saved", exact: true }).selectOption({ label: `${presetName} — vanilla · current data` });
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await page.getByText(`Loaded ${presetName}.`, { exact: true }).waitFor();
  await expect(page.locator(".result-row-full")).toHaveCount(1);
  const reloadedRow = page.locator(".result-row-full").first();
  await expect(reloadedRow.locator(".weapon-cell strong")).toHaveText("Uchigatana");
  await expect(reloadedRow.locator(".setup-cell strong")).toHaveText("Blood");
  await expect(reloadedRow.locator(".setup-cell > small")).toHaveText("Seppuku");
  await expect(reloadedRow.getByRole("gridcell").nth(3)).toHaveText("+25");
  await expect(reloadedRow.locator(".row-combat-stats")).toHaveText("STR 13 / DEX 19 / INT 9 / FAI 8 / ARC 63");
  await expect(reloadedRow.locator(".ar-status-cell strong")).toHaveText("665");
  await expect(page.locator('[aria-label="Bleed buildup: 99"]')).toBeVisible();
  await expect(page.locator(".selected-build")).toContainText("Blood / Seppuku / +25");
  await expect(page.locator(".detail-block").filter({ hasText: "Combat Stats" }).locator("strong")).toHaveText("STR 13 / DEX 19 / INT 9 / FAI 8 / ARC 63");
  const reloadedState = await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const state = useDesktopStore.getState();
    return { selected: state.selected, request: state.request };
  });
  expect(reloadedState.selected).toEqual(returnedResult);
  expect(reloadedState.request).toMatchObject({
    lockStr: 13, lockDex: 19, lockInt: 9, lockFai: 8, lockArc: 63,
    weaponName: "Uchigatana", affinity: "Blood", aowName: "Seppuku", exactUpgrade: true,
  });
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await page.getByText("Comparison current", { exact: true }).waitFor();
  await expect(page.getByRole("region", { name: "AR / Bleed tradeoffs" })).toContainText("All five combat locks are active, so there is no stat allocation left to vary.");
});

test("singleton frontier explains tied allocations", async ({ page }) => {
  await openCompare(page);
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const original = api.arBleedFrontier;
    api.arBleedFrontier = async (...args) => [(await original(...args))[0]];
  });
  const section = page.getByRole("region", { name: "AR / Bleed tradeoffs" });
  await expect(section).not.toContainText("Only one non-dominated AR / bleed outcome exists under these constraints; other allocations may tie.");
  await section.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(section.getByRole("status")).toHaveText("1 exact trade-off point ready.");
  await expect(section).toContainText("Only one non-dominated AR / bleed outcome exists under these constraints; other allocations may tie.");
  await expect(section.locator(".tradeoff-header")).toContainText("world settings. Only one");
});

test("leaving Compare aborts a pending frontier and discards its late result", async ({ page }) => {
  await openCompare(page);
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    (window as any).frontierProbe = { aborted: false };
    api.arBleedFrontier = (_base, _selected, signal) => new Promise(resolve => {
      (window as any).frontierProbe.finish = resolve;
      signal?.addEventListener("abort", () => { (window as any).frontierProbe.aborted = true; });
    });
  });
  await page.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(page.locator(".loadout-tradeoffs").getByRole("status")).toContainText("Calculating");
  await page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true }).click();
  expect(await page.evaluate(() => (window as any).frontierProbe.aborted)).toBe(true);
  await page.evaluate(() => (window as any).frontierProbe.finish([]));
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.locator(".loadout-tradeoffs").getByRole("status")).toContainText("Ready to calculate");
  await expect(page.locator(".tradeoff-shortlist")).toHaveCount(0);
});
