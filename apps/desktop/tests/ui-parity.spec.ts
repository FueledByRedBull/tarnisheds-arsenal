import { expect, test } from "@playwright/test";

test("profile switch isolates results and explains Convergence coverage", async ({ page }) => {
  await page.goto("/");
  const profiles = page.getByRole("radiogroup", { name: "Game profile" });
  await expect(profiles.getByRole("radio", { name: /Vanilla/ })).toHaveAttribute("aria-checked", "true");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  await profiles.getByRole("radio", { name: /Convergence/ }).click();
  await expect(profiles.getByRole("radio", { name: /Convergence/ })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
  await page.locator(".profile-coverage summary").click();
  await expect(
    page.getByText(/Ammo weapons and AoW hit\/route damage remain unsupported/),
  ).toBeVisible();
  await expect(page.getByText("4 ranked rows")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "AoW First Hit" })).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "Class", exact: true })).toHaveValue("Custom stats");
  await expect(page.getByRole("button", { name: "Optimize class" })).toBeDisabled();
  await expect(page.getByRole("textbox", { name: "Stat total", exact: true })).toBeVisible();
  await expect(page.getByRole("checkbox", { name: "Use entered combat stats exactly" })).toBeChecked();
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("1 ranked rows")).toBeVisible();
  const rawSkill = page.locator(".result-row-full").first().getByRole("gridcell").nth(5);
  await expect(rawSkill).toHaveText("Unavailable");
  await page.locator(".result-row-full").first().click();
  await expect(page.locator(".metric-tile").filter({ hasText: "AoW model" })).toContainText("Unavailable");
  await expect(page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true })).toBeDisabled();

  await expect(page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true })).toBeDisabled();
  await expect.poll(() => page.evaluate(() => localStorage.getItem("tarnisheds-arsenal.gameProfile.v1"))).toBe("convergence");
  await profiles.getByRole("radio", { name: /Vanilla/ }).click();
  await expect(page.getByRole("combobox", { name: "Class", exact: true })).toHaveValue("Samurai");
  await expect(page.getByRole("button", { name: "Optimize class" })).toBeEnabled();
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await expect(rawSkill).not.toContainText("Unavailable");
  await expect(rawSkill).toContainText("First");
  await page.locator(".result-row-full").first().click();
  await expect(page.locator(".metric-tile").filter({ hasText: "Raw AoW" })).not.toContainText("Unavailable");

});

test("fixed skill controls use the same native selection in Rankings and Compare", async ({ page }) => {
  await page.goto("/");
  await chooseSearchableOption(page, "Weapon", "Ancient Meteoric Ore Greatsword");
  const rankingSkill = page.getByRole("combobox", { name: "AoW (fixed)", exact: true });
  await expect(rankingSkill).toHaveValue("White Light Charge");
  await expect(rankingSkill).toBeDisabled();
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("1 ranked rows")).toBeVisible();
  await expect(page.getByRole("grid", { name: "Ranked builds" })).toContainText("White Light Charge");
  await chooseSearchableOption(page, "Weapon", "Open");
  await page.getByRole("button", { name: "Update Results", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.locator(".result-row-full").first().click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await chooseSearchableOption(page, "Compare Weapon", "Ancient Meteoric Ore Greatsword");
  const comparisonSkill = page.getByRole("combobox", { name: "Compare AoW (fixed)", exact: true });
  await expect(comparisonSkill).toHaveValue("White Light Charge");
  await expect(comparisonSkill).toBeDisabled();
  await page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(comparisonSkill).toHaveValue("White Light Charge");
});

test("session-driven search, lock, compare, paths, and affinity watch", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/");

  await expect(page.getByText("No rankings loaded", { exact: true })).toBeVisible();
  await expect(page.locator(".top-cards")).toHaveCount(0);

  await expect(page.getByRole("textbox", { name: "Level", exact: true })).toHaveValue("9");
  await expect(page.getByText("Redistrib", { exact: true })).toBeVisible();
  await page.getByRole("combobox", { name: "Class" }).click();
  await expect(page.getByRole("option", { name: "Wretch" })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("130");
  await expect(page.getByRole("spinbutton", { name: "STR", exact: true })).toHaveValue("130");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).press("Enter");
  await expect(page.getByRole("spinbutton", { name: "STR", exact: true })).toHaveValue("99");
  await page.getByRole("combobox", { name: "Class" }).click();
  await page.getByRole("option", { name: "Vagabond" }).click();
  await expect(page.getByRole("spinbutton", { name: "VIG" })).toHaveValue("15");
  await expect(page.getByRole("spinbutton", { name: "STR", exact: true })).toHaveValue("14");
  await expect(page.getByRole("spinbutton", { name: "DEX", exact: true })).toHaveValue("13");
  await expect(page.getByRole("textbox", { name: "Level", exact: true })).toHaveValue("9");
  await toggleMultiSelectOption(page, "Weapon Type", "Great Katana");
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("Great Katana");
  await page.getByRole("button", { name: "Clear all" }).click();
  await chooseSearchableOption(page, "Weapon", "Zweihander");
  await expect(page.getByRole("combobox", { name: "Weapon", exact: true })).toHaveValue("Zweihander");
  await chooseSearchableOption(page, "Weapon", "Open");
  await chooseSearchableOption(page, "AoW", "Bloodhound's Step");
  await expect(page.getByRole("combobox", { name: "AoW" })).toHaveValue("Bloodhound's Step");
  await chooseSearchableOption(page, "AoW", "Automatic (best legal skill)");

  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  const firstRankedRow = page.locator(".result-row-full").first();
  await expect(firstRankedRow).toContainText("Uchigatana");
  await expect(firstRankedRow.getByRole("list", { name: "Attack rating split" })).toHaveCount(0);
  await expect(firstRankedRow.getByRole("gridcell").nth(4)).toHaveText("700");
  await expect(firstRankedRow.locator(".row-combat-stats")).toHaveText("STR 13 / DEX 22 / INT 9 / FAI 8 / ARC 60");
  const rowScaling = firstRankedRow.getByRole("list", { name: "Attribute scaling" });
  await expect(rowScaling.getByRole("listitem", { name: "Strength scaling: C", exact: true })).toBeVisible();
  await expect(rowScaling.getByRole("listitem", { name: "Arcane scaling: D", exact: true })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "AR", exact: true })).toBeVisible();
  await page.getByText("Model coverage and assumptions").click();
  await expect(page.getByText("Attack rating is calculated before enemy defense and negation.")).toBeVisible();
  await expect(page.getByText(/Temporary buff stacking is not a universal layer/)).toBeVisible();
  await expectRankingsBoardToFit(page);
  await expect(page.getByRole("button", { name: "Show lowest rank first" })).toBeVisible();
  await page.getByRole("button", { name: "Show lowest rank first" }).click();
  await expect(page.locator(".result-row-full").first().locator(".rank-cell")).toHaveText("4");
  await page.getByRole("button", { name: "Show best rank first" }).click();
  await expect(page.locator(".result-row-full").first().locator(".rank-cell")).toHaveText("1");
  await expect.poll(() => page.locator(".result-row-full").first().locator("[role=gridcell]").nth(1).evaluate(
    (node) => getComputedStyle(node).position,
  )).toBe("sticky");
  await page.getByRole("row", { name: "Select Uchigatana, Occult, rank 2", exact: true }).click();
  await expect(page.locator(".selected-build")).toContainText("Occult / Seppuku / +25");
  await expect(page.locator(".inspector .weapon-poise-detail")).toContainText("Weight 7.0 · 5 mapped poise moves · 1H");
  await expect(page.locator(".inspector .weapon-poise-detail")).toContainText("PvE stance / poise damage");
  await expect(page.locator(".inspector .weapon-poise-detail")).toContainText("R15");
  await expect(page.locator(".inspector .weapon-poise-detail")).toContainText("Jumping R17.5");
  await expect(page.locator(".inspector .weapon-poise-detail")).toContainText("Jumping R220");

  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("13");
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await expect(page.getByText("Inputs changed")).toBeVisible();
  await page.getByRole("button", { name: "Update Results" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await expect.poll(() => page.locator(".result-row-full").first().evaluate(
    (node) => node.getBoundingClientRect().height,
  )).toBeLessThan(145);

  await page.locator(".result-row-full").first().getByRole("button", { name: /^Lock / }).click();
  await expect(page.getByText("Exact upgrade and stat locks active")).toBeVisible();
  await expect(page.locator(".active-lock-warning")).toContainText("Changing class or loadout keeps these locks");
  await expect(page.getByText("Blood / Seppuku / +25").first()).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(page.getByText("Selected baseline versus current ranked rivals")).toBeVisible();
  const selectedLane = page.locator(".compare-lane", { hasText: "Selected" });
  await expect(selectedLane).toContainText("Uchigatana");
  await expect(selectedLane).toContainText("Blood / Seppuku");
  const selectedScaling = selectedLane.getByRole("list", { name: "Attribute scaling" });
  await expect(selectedScaling.getByRole("listitem", { name: "Strength scaling: C" })).toBeVisible();
  await expect(selectedScaling.getByRole("listitem", { name: "Dexterity scaling: B" })).toBeVisible();
  await expect(selectedScaling.getByRole("listitem", { name: "Arcane scaling: D" })).toBeVisible();
  await chooseSearchableOption(page, "Compare Weapon", "Uchigatana");
  await toggleMultiSelectOption(page, "Compare Affinity", "Occult");
  const targetLane = page.locator(".compare-lane").nth(1);
  await expect(targetLane).toContainText("Occult / Seppuku");
  await expect(targetLane).toContainText("ARC 60");
  await expect(targetLane).toContainText("AR 670");
  const targetScaling = targetLane.getByRole("list", { name: "Attribute scaling" });
  await expect(targetScaling.getByRole("listitem", { name: "Strength scaling: E" })).toBeVisible();
  await expect(targetScaling.getByRole("listitem", { name: "Dexterity scaling: D" })).toBeVisible();
  await expect(targetScaling.getByRole("listitem", { name: "Arcane scaling: B" })).toBeVisible();
  await page.locator(".matrix-toolbar").getByRole("button", { name: "+25" }).click();
  await expect(page.locator(".metric-matrix").getByText("+25")).toBeVisible();
  await expect(page.getByText("+25").first()).toBeVisible();
  await expect.poll(() => page.locator(".matrix-wrap").evaluate((node) => node.scrollWidth > node.clientWidth)).toBe(true);
  await expect.poll(() => page.evaluate(() => document.body.scrollWidth <= window.innerWidth + 2)).toBe(true);

  await page.getByRole("navigation").getByRole("button", { name: "Paths" }).click();
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("90");
  await page.getByRole("button", { name: "Trace paths" }).click();
  await expect(page.getByText("Selected").first()).toBeVisible();
  await expect(page.getByText("Compare").first()).toBeVisible();
  await expect(page.locator(".path-chart")).toContainText("Stat breakpoint");
  await expect(page.locator(".analysis-progress")).toHaveAttribute("data-analysis-status", "completed");
  await page.getByRole("button", { name: "Envelope", exact: true }).click();
  await expect(page.getByRole("button", { name: "Envelope", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".analysis-progress")).toHaveAttribute("data-analysis-status", "ready");
  await expect(page.locator(".path-chart .spark-line")).toHaveCount(0);
  await page.getByRole("button", { name: "Trace paths", exact: true }).click();
  await expect(page.locator(".analysis-progress")).toHaveAttribute("data-analysis-status", "completed");
  await expect.poll(() => page.locator(".path-chart .spark-line").first().evaluate(
    (node) => node.scrollWidth <= node.clientWidth + 1,
  )).toBe(true);
  await expect.poll(() => page.evaluate(
    () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
  )).toBe(true);
  await expect.poll(() => page.locator(".paths-panel").evaluate((node) => getComputedStyle(node).overflowY)).toBe("auto");

  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch" }).click();
  await page.getByRole("button", { name: "Watch affinities" }).click();
  const affinityRankings = page.getByRole("grid", { name: "Affinity watch rankings" });
  await expect(affinityRankings).toContainText("Keen");
  await expect(page.locator(".affinity-chart")).toContainText("Best-affinity crossover");
  await expect(page.locator(".affinity-plot svg")).toBeVisible();
  await expect(page.locator(".affinity-chart .spark-line")).toHaveCount(0);
  await expect(affinityRankings).toContainText("Occult");
  await expect(page.locator(".analysis-progress")).toHaveAttribute("data-analysis-status", "completed");
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("5");
  await expect(page.locator(".analysis-progress")).toHaveAttribute("data-analysis-status", "ready");
});

test("somber-only exact search uses the somber upgrade cap", async ({ page }) => {
  await page.goto("/");

  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("60");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).press("Enter");
  await expect(page.getByRole("textbox", { name: "Level", exact: true })).toHaveValue("57");
  await expect(page.getByRole("spinbutton", { name: "Standard Upgrade" })).toHaveValue("25");
  await expect(page.getByRole("spinbutton", { name: "Somber Upgrade" })).toHaveValue("10");
  await page.getByRole("button", { name: "Use exact levels" }).click();
  await page.getByText("Advanced", { exact: true }).click();
  await chooseSearchableOption(page, "Somber", "Somber Only");

  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("1 ranked row")).toBeVisible();
  await expect(page.getByRole("grid", { name: "Ranked builds" })).toContainText("Somber");
  await expect(page.getByRole("grid", { name: "Ranked builds" })).toContainText("Ancient Meteoric Ore Greatsword");
  await expect(page.getByRole("grid", { name: "Ranked builds" })).toContainText("+10");
});

test("original weapon type and affinity selectors support multiple checks", async ({ page }) => {
  await page.goto("/");

  await chooseSearchableOption(page, "Weapon", "Uchigatana");
  await expect(page.getByRole("combobox", { name: "Weapon", exact: true })).toHaveValue("Uchigatana");
  await expect(page.getByText(/Weapon types & affinities/)).toHaveCount(0);

  await toggleMultiSelectOption(page, "Weapon Type", "Katana");
  await expect(page.getByRole("combobox", { name: "Weapon", exact: true })).toHaveValue("Open");
  await toggleMultiSelectOption(page, "Weapon Type", "Colossal Sword");
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("Katana");
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("Colossal Sword");
  await page.getByRole("button", { name: "Weapon Type" }).press("Escape");

  await toggleMultiSelectOption(page, "Affinity", "Blood");
  await toggleMultiSelectOption(page, "Affinity", "Keen");
  await expect(page.getByRole("button", { name: "Affinity", exact: true })).toContainText("Blood");
  await expect(page.getByRole("button", { name: "Affinity", exact: true })).toContainText("Keen");
  await page.getByRole("button", { name: "Affinity", exact: true }).press("Escape");
  await page.getByRole("button", { name: "Reset weapon filters" }).click();
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("All");
  await expect(page.getByRole("button", { name: "Affinity", exact: true })).toContainText("All");
  await expect(page.getByRole("button", { name: "Reset weapon filters" })).toBeDisabled();

  await chooseSearchableOption(page, "Weapon", "Uchigatana");
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("All");
});

test("weapon filters cycle include, exclude, and neutral", async ({ page }) => {
  await page.goto("/");

  await toggleMultiSelectOption(page, "Weapon Type", "Katana");
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("Katana");
  await toggleMultiSelectOption(page, "Weapon Type", "Katana");
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("Not Katana");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("1 ranked row")).toBeVisible();

  await toggleMultiSelectOption(page, "Weapon Type", "Katana");
  await expect(page.getByRole("button", { name: "Weapon Type" })).toContainText("All");
});

test("starting class optimization and stat reset keep the level budget valid", async ({ page }) => {
  await page.goto("/");

  const stats = ["VIG", "MND", "END", "STR", "DEX", "INT", "FAI", "ARC"];
  const before = await Promise.all(stats.map((stat) => page.getByRole("spinbutton", { name: stat, exact: true }).inputValue()));
  await page.getByRole("button", { name: "Optimize class" }).click();
  await expect(page.getByRole("combobox", { name: "Class" })).toHaveValue("Samurai");
  await expect(page.getByRole("textbox", { name: "Level", exact: true })).toHaveValue("9");
  for (const [index, stat] of stats.entries()) {
    await expect(page.getByRole("spinbutton", { name: stat, exact: true })).toHaveValue(before[index]);
  }

  await chooseSearchableOption(page, "Class", "Wretch");
  await page.getByRole("button", { name: "Reset stats" }).click();
  await expect(page.getByRole("textbox", { name: "Level", exact: true })).toHaveValue("1");
  for (const stat of ["VIG", "MND", "END", "STR", "DEX", "INT", "FAI", "ARC"]) {
    await expect(page.getByRole("spinbutton", { name: stat, exact: true })).toHaveValue("10");
  }
});

test("compare combines multiple types and affinities with Smithing and Somber toggles", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await page.locator(".result-row-full").first().click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();

  await expect.poll(async () => {
    const header = await page.locator(".compare-workspace-header").boundingBox();
    const summary = await page.locator(".compare-workspace-header .selected-summary").boundingBox();
    return Boolean(header && summary && summary.y + summary.height < header.y + header.height - 3);
  }).toBe(true);
  await expect.poll(() => page.locator(
    ".compare-toolbar .searchable-select > input, .compare-toolbar .checkbox-multi-trigger, .compare-reinforcement label",
  ).evaluateAll((controls) => {
    const boxes = controls.map((control) => control.getBoundingClientRect());
    return boxes.length === 6
      && controls.every((control) => {
        const box = control.getBoundingClientRect();
        const parent = control.parentElement!.getBoundingClientRect();
        return box.width > 0 && box.right <= parent.right + 1;
      });
  })).toBe(true);

  await toggleMultiSelectOption(page, "Compare Type", "Great Katana");
  await toggleMultiSelectOption(page, "Compare Type", "Katana");
  await toggleMultiSelectOption(page, "Compare Affinity", "Unique");
  await toggleMultiSelectOption(page, "Compare Affinity", "Keen");
  const reinforcement = page.getByRole("group", { name: "Compare Reinforcement" });
  await reinforcement.getByRole("checkbox", { name: "Smithing" }).uncheck();

  const targetLane = page.locator(".compare-lane").nth(1);
  await expect(targetLane).toContainText("Ancient Meteoric Ore Greatsword");
  await expect(targetLane).toContainText("Unique");
  await expect(page.getByText("Best Great Katana + Katana · Keen + Unique · Somber", { exact: true })).toBeVisible();

  await reinforcement.getByRole("checkbox", { name: "Somber" }).uncheck();
  await expect(targetLane).toContainText("Select Smithing, Somber, or both");
  await reinforcement.getByRole("checkbox", { name: "Smithing" }).check();
  await expect(targetLane).toContainText("Uchigatana");
  await expect(targetLane).toContainText("Keen");
});

test("comparison targets survive pin, clear, type search, and workspace navigation", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  const occult = page.locator(".result-row-full").nth(1);
  const keen = page.locator(".result-row-full").nth(2);
  await occult.click();
  await keen.getByRole("button", { name: /^Compare / }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(page.getByText("Selected baseline versus 1 pinned target")).toBeVisible();
  await expect(page.locator(".compare-lane", { hasText: "Pinned #1" })).toContainText("Keen");

  await page.getByRole("button", { name: "Clear 1 pinned target" }).click();
  await expect(page.locator(".compare-lane", { hasText: "Selected" })).toContainText("Occult");
  await expect(page.getByText("Selected baseline versus current ranked rivals")).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "Rankings" }).click();
  await occult.getByRole("button", { name: /^Compare / }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(page.getByText(/pinned target is already the selected baseline/i)).toBeVisible();

  await toggleMultiSelectOption(page, "Compare Type", "Great Katana");
  await expect(page.getByRole("button", { name: "Clear 1 pinned target" })).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "Compare Weapon" })).toHaveValue("Best Great Katana");
  const bestType = page.locator(".compare-lane", { hasText: "Best Great Katana" });
  await expect(bestType).toContainText("Ancient Meteoric Ore Greatsword");

  await toggleMultiSelectOption(page, "Compare Type", "Great Katana");
  await toggleMultiSelectOption(page, "Compare Type", "Katana");
  await expect(page.locator(".compare-lane", { hasText: "Best Katana" })).toContainText("Uchigatana");
  await toggleMultiSelectOption(page, "Compare Type", "Katana");
  await toggleMultiSelectOption(page, "Compare Type", "Great Katana");
  await toggleMultiSelectOption(page, "Compare Type", "Great Katana");
  await expect(bestType).toContainText("Ancient Meteoric Ore Greatsword");

  await page.getByRole("navigation").getByRole("button", { name: "Rankings" }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(bestType).toContainText("Ancient Meteoric Ore Greatsword");

  await page.getByRole("navigation").getByRole("button", { name: "Rankings" }).click();
  await keen.getByRole("button", { name: /^Compare / }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(page.getByRole("button", { name: "Compare Type" })).toContainText("All");
  await expect(page.getByText("Selected baseline versus 1 pinned target")).toBeVisible();
  await expect(page.locator(".compare-lane", { hasText: "Pinned #1" })).toContainText("Keen");
  await expect(page.locator(".compare-lane", { hasText: "Selected" })).toContainText("Occult");
});

test("ranked rows select the exact build with mouse and keyboard", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  const fourthRow = page.locator(".result-row-full").nth(3);
  const weaponName = (await fourthRow.locator(".weapon-cell strong").textContent())?.trim();
  expect(weaponName).toBeTruthy();

  await fourthRow.click();
  await expect(fourthRow).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".selected-build strong")).toHaveText(weaponName ?? "");

  await page.getByRole("button", { name: "Save build", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Name", exact: true })).toBeFocused();

  const thirdRow = page.locator(".result-row-full").nth(2);
  await thirdRow.focus();
  await thirdRow.press("Enter");
  await expect(thirdRow).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".selected-build")).toContainText("Keen / Seppuku / +25");
  const hoveredRow = page.locator(".result-row-full").nth(1);
  await hoveredRow.hover();
  await expect.poll(() => hoveredRow.evaluate((row) => {
    const background = getComputedStyle(row).backgroundColor;
    return [...row.children].slice(0, 2).every(cell => getComputedStyle(cell).backgroundColor === background);
  })).toBe(true);
  await expect.poll(() => hoveredRow.locator(".setup-cell .row-detail-label").evaluate((label) => {
    const heading = label.getBoundingClientRect();
    const grades = label.nextElementSibling!.getBoundingClientRect();
    return getComputedStyle(label).textAlign === "center"
      && Math.abs(heading.left + heading.width / 2 - grades.left - grades.width / 2) < 1;
  })).toBe(true);
});

test("saved family filters can be cleared without editing the saved build", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Save new" }).click();
  await expect(page.getByText(/Saved Build Preset/)).toBeVisible();
  await page.evaluate(() => {
    const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."));
    if (!key) throw new Error("saved preset was not created");
    const preset = JSON.parse(localStorage.getItem(key) ?? "null");
    preset.request.filters.entries = [{ dimension: "weapon_family", id: "weapon:1001000", mode: "exclude" }];
    localStorage.setItem(key, JSON.stringify(preset));
  });
  await page.reload();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  const reset = page.getByRole("button", { name: "Reset weapon filters" });
  await expect(reset).toBeEnabled();
  await reset.click();
  await expect(reset).toBeDisabled();
  expect(await page.evaluate(() => {
    const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."));
    return JSON.parse(localStorage.getItem(key!)!).request.filters.entries;
  })).toEqual([{ dimension: "weapon_family", id: "weapon:1001000", mode: "exclude" }]);
});

test("stale saved builds offer explicit input-only loading or recompute migration", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.getByRole("button", { name: "Save new" }).click();
  await expect(page.getByText(/Saved Build Preset/)).toBeVisible();
  await expect(page.locator(".saved-build-status")).toContainText(
    "Current · profile vanilla · dataset vanilla-1.17 · schema 4",
  );

  await page.evaluate(() => {
    const presetKey = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."));
    if (!presetKey) throw new Error("saved preset was not created");
    const preset = JSON.parse(localStorage.getItem(presetKey) ?? "null");
    preset.dataVersion = "1:old-dataset:old-model";
    localStorage.setItem(presetKey, JSON.stringify(preset));
    const indexKey = "tarnisheds-arsenal.savedBuildIndex.v1";
    const index = JSON.parse(localStorage.getItem(indexKey) ?? "null");
    index.builds[0].dataVersion = preset.dataVersion;
    localStorage.setItem(indexKey, JSON.stringify(index));
  });
  await page.reload();

  await expect(page.getByText(/Stale.*solved rows are discarded/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Load inputs only" })).toBeVisible();
  await page.getByRole("button", { name: "Migrate data" }).click();
  await expect(page.getByText(/Migrated Build Preset/)).toBeVisible();
  await expect(page.getByText(/Current.*dataset/)).toBeVisible();
  await expect(page.locator(".selected-build")).toContainText("Uchigatana");
});

test("reduced-motion preference disables decorative motion", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");

  const animationDurationMs = await page.locator(".workspace-stage").evaluate(
    (node) => {
      const duration = getComputedStyle(node).animationDuration;
      return Number.parseFloat(duration) * (duration.endsWith("ms") ? 1 : 1000);
    },
  );
  expect(animationDurationMs).toBeLessThanOrEqual(0.01);
});

test("searchable selects expose keyboard and screen-reader state", async ({ page }) => {
  await page.goto("/");
  const classField = page.getByRole("combobox", { name: "Class" });

  await classField.focus();
  await classField.press("ArrowDown");
  await expect(classField).toHaveAttribute("aria-expanded", "true");
  await expect(classField).toHaveAttribute("aria-activedescendant", /option-/);
  await expect(page.locator(".select-status").filter({ hasText: /matches/ }).first()).toBeVisible();
  await classField.press("Escape");
  await expect(classField).toHaveValue("Samurai");

  await classField.fill("Wre");
  await expect(page.locator(".select-status").filter({ hasText: "1 match" }).first()).toBeVisible();
  await classField.press("Enter");
  await expect(classField).toHaveValue("Wretch");

  await classField.fill("not-a-class");
  await expect(page.getByText("No matches").first()).toBeVisible();
  await classField.press("Escape");
  await expect(classField).toHaveValue("Wretch");

  await classField.fill("Vagabond");
  await classField.press("Tab");
  await expect(classField).toHaveValue("Vagabond");
});

test("1366px layout survives 125 percent text scaling and long labels", async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 768 });
  await page.goto("/");
  await page.evaluate(() => { document.documentElement.style.fontSize = "17.5px"; });
  await chooseSearchableOption(page, "Weapon", "Ancient Meteoric Ore Greatsword");

  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeVisible();
  await expect(page.getByRole("navigation")).toBeVisible();
  await expect.poll(() => page.evaluate(() => document.body.scrollWidth <= window.innerWidth + 2)).toBe(true);
  await expect.poll(() => page.evaluate(() => document.body.scrollHeight <= window.innerHeight + 2)).toBe(true);
});

async function chooseSearchableOption(page: import("@playwright/test").Page, label: string, option: string) {
  const field = page.getByRole("combobox", { name: label, exact: true });
  await field.fill(option);
  await page.keyboard.press("Enter");
}

test("analysis controls align inputs with buttons and path levels compare side by side in pages", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  const nav = page.getByRole("navigation");
  await nav.getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  for (const width of [1200, 1294, 1650]) {
    await page.setViewportSize({ width, height: 856 });
    for (const workspace of ["Paths", "Affinity Watch"]) {
      await nav.getByRole("button", { name: workspace, exact: true }).click();
      await expect.poll(() => page.locator(".analysis-workspace-header .header-controls").evaluate((controls) => {
        const group = controls.getBoundingClientRect();
        const header = controls.parentElement!.getBoundingClientRect();
        const headerStyle = getComputedStyle(controls.parentElement!);
        const input = controls.querySelector("input")!;
        const field = input.getBoundingClientRect();
        return Math.abs(group.right - (header.right - parseFloat(headerStyle.paddingRight) - parseFloat(headerStyle.borderRightWidth))) < 1
          && [...controls.querySelectorAll("button")].every(button => {
            const bounds = button.getBoundingClientRect();
            return Math.abs(bounds.top - field.top) < 1 && Math.abs(bounds.bottom - field.bottom) < 1;
          })
          && getComputedStyle(input).textAlign === "center";
      })).toBe(true);
    }
  }
  await page.setViewportSize({ width: 1294, height: 856 });
  await nav.getByRole("button", { name: "Paths", exact: true }).click();
  await page.getByRole("button", { name: "Trace paths", exact: true }).click();
  await expect(page.locator(".analysis-progress")).toHaveAttribute("data-analysis-status", "completed");
  await page.evaluate(async () => {
    const modulePath = "/src/lib/state.ts";
    const { useDesktopStore } = await import(modulePath);
    const state = useDesktopStore.getState();
    state.setPaths(state.paths.map((path: any, lane: number) => ({
      ...path,
      steps: Array.from({ length: 41 }, (_, index) => ({
        ...path.steps[0], level: 9 + index, metric: index === 2 ? null : 500 + lane * 100 + (index === 1 ? 0 : index * 2),
        addedStat: index && index !== 2 ? "dex" : null,
        requirementGap: index === 2 ? 3 : 0,
      })),
    })), state.pathSignature);
  });
  const grid = page.getByRole("grid", { name: "Path steps" });
  await expect(grid.getByRole("row")).toHaveCount(11);
  await expect(grid.getByRole("columnheader")).toHaveCount(3);
  await expect(grid.getByRole("row").nth(1).getByRole("gridcell").nth(0)).toHaveText("9");
  await expect(grid.getByRole("row").nth(1)).toContainText("500.0 AR");
  await expect(grid.getByRole("row").nth(1)).toContainText("600.0 AR");
  await expect(grid.getByRole("row").nth(1)).toContainText("Starting stats");
  await expect(grid.getByRole("row").nth(1)).not.toContainText("Gain -");
  await expect(grid.getByRole("row").nth(2)).toContainText("Gain 0.0 | Added DEX");
  await expect(grid.getByRole("row").nth(3)).toContainText("Gain unavailable | No stat added | Requirement gap 3");
  await expect(page.getByRole("combobox", { name: "Path level range" }).locator('option[value="0"]')).toHaveText("9–18");
  await expect(page.locator(".path-steps")).not.toContainText("\uFFFD");
  await expect(page.getByRole("button", { name: "Previous levels" })).toBeDisabled();
  await page.getByRole("button", { name: "Next levels" }).click();
  await expect(grid.getByRole("row").nth(1).getByRole("gridcell").nth(0)).toHaveText("19");
  await expect(grid.getByRole("row").nth(1)).toContainText("Gain 2.0");
  await page.getByRole("combobox", { name: "Path level range" }).selectOption("4");
  await expect(grid.getByRole("row")).toHaveCount(2);
  await expect(grid.getByRole("row").nth(1).getByRole("gridcell").nth(0)).toHaveText("49");
  await expect(page.getByRole("button", { name: "Next levels" })).toBeDisabled();
  await expect.poll(() => grid.evaluate(node => node.scrollWidth <= node.clientWidth + 1)).toBe(true);
  await page.getByRole("button", { name: "Envelope", exact: true }).click();
  await expect(grid).toHaveCount(0);
});

async function toggleMultiSelectOption(page: import("@playwright/test").Page, label: string, option: string) {
  const group = page.getByRole("group", { name: label, exact: true });
  if (!await group.isVisible()) await page.getByRole("button", { name: label, exact: true }).click();
  const checkbox = group.getByRole("checkbox", { name: new RegExp(`^${option.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`) });
  await checkbox.click();
}

async function expectRankingsBoardToFit(page: import("@playwright/test").Page) {
  const board = page.getByRole("grid", { name: "Ranked builds" });
  await expect.poll(() => board.evaluate((node) => node.scrollWidth <= node.clientWidth + 2)).toBe(true);
}

for (const [width, height] of [[923, 789], [1200, 720], [1366, 768], [1650, 950]]) {
  test(`stacked headers do not overlap with 50 rankings at ${width}x${height}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.evaluate(async () => {
      const modulePath = "/src/lib/state.ts";
      const { useDesktopStore } = await import(modulePath);
      const rows = useDesktopStore.getState().rows;
      useDesktopStore.setState({ rows: Array.from({ length: 50 }, (_, index) => rows[index % rows.length]) });
    });
    await expect(page.getByText("50 ranked rows")).toBeVisible();
    const profileFits = () => page.locator(".profile-bar").evaluate((profile) => {
      const bottom = profile.getBoundingClientRect().bottom;
      return [...profile.children].every(child => child.getBoundingClientRect().bottom <= bottom - 5)
        && document.querySelector(".workspace-tabs")!.getBoundingClientRect().top >= bottom + 7;
    });
    await expect.poll(profileFits).toBe(true);
    await expect.poll(() => page.locator(".query-summary").evaluate((query) =>
      document.querySelector(".mechanics-glossary")!.getBoundingClientRect().top - query.getBoundingClientRect().bottom,
    )).toBeGreaterThanOrEqual(8);
    await page.locator(".mechanics-glossary summary").click();
    await expect.poll(() => page.locator(".mechanics-glossary dl").evaluate((list) =>
      list.scrollWidth <= list.clientWidth + 1,
    )).toBe(true);
    await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await expect.poll(profileFits).toBe(true);
  });
}

for (const [width, height] of [[1200, 720], [1366, 768], [1650, 950]]) {
  test(`frontend hierarchy and controls fit at ${width}x${height}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.goto("/");
    const nav = page.getByRole("navigation");
    await expect(nav.getByRole("button", { name: "Rankings", exact: true })).toHaveAttribute("aria-current", "page");
    await expect(page.getByRole("button", { name: "Max AR", exact: true })).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByRole("button", { name: "Auto", exact: true })).toHaveAttribute("aria-pressed", "true");
    await expect.poll(() => page.locator(".level-strip").evaluate((group) => {
      const box = group.getBoundingClientRect();
      return [...group.children].every((child) => child.getBoundingClientRect().right <= box.right + 1);
    })).toBe(true);
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await expect(page.locator(".top-cards")).toHaveCount(0);
    await expect.poll(() => page.locator(".result-row-full").first().evaluate((row) =>
      [...row.querySelectorAll(".row-combat-stats, .scaling-token-grid")].every(node => node.scrollWidth <= node.clientWidth + 1),
    )).toBe(true);
    await expect.poll(() => page.locator(".result-row-full").first().evaluate((row) => row.getBoundingClientRect().height)).toBeLessThan(145);
    await expect.poll(() => page.locator(".inspector").evaluate((inspector) => {
      const selected = inspector.querySelector(".selected-build")!;
      const budget = [...inspector.querySelectorAll(".detail-block")].find(n => n.textContent?.includes("Stat Budget"))!;
      return selected.getBoundingClientRect().top < budget.getBoundingClientRect().top;
    })).toBe(true);
    await nav.getByRole("button", { name: "Compare", exact: true }).click();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Search", exact: true })).toHaveCount(0);
    await expect.poll(() => page.locator(".compare-reinforcement").evaluate((group) => {
      const box = group.getBoundingClientRect();
      return [...group.querySelectorAll("label")].every(label => label.getBoundingClientRect().right <= box.right + 1);
    })).toBe(true);
    await expect.poll(() => page.locator(".compare-deltas").evaluate((table) => {
      const details = document.querySelector(".compare-build-details")!;
      return table.getBoundingClientRect().top < details.getBoundingClientRect().top;
    })).toBe(true);
    await page.getByRole("radio", { name: /Convergence/ }).click();
    await expect(page.getByRole("textbox", { name: "Stat total", exact: true })).toBeVisible();
    await expect(page.getByText(/\d+ free points/)).toHaveCount(0);
    await expect(page.getByText("Redistrib", { exact: true })).toHaveCount(0);
    await expect(page.getByText("Open or partial locks", { exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /Try .* example/ })).toHaveCount(0);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  });
}
