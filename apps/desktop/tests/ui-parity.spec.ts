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
  await expect(page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true })).toBeDisabled();
  await expect.poll(() => page.evaluate(() => localStorage.getItem("tarnisheds-arsenal.gameProfile.v1"))).toBe("convergence");
  await profiles.getByRole("radio", { name: /Vanilla/ }).click();
  await expect(page.getByRole("combobox", { name: "Class", exact: true })).toHaveValue("Samurai");
  await expect(page.getByRole("button", { name: "Optimize class" })).toBeEnabled();
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

  const emptyPodiumCards = page.locator(".top-card-empty");
  await expect(emptyPodiumCards).toHaveCount(3);
  await expect(page.locator(".top-card.active")).toHaveCount(0);
  await expect.poll(async () => emptyPodiumCards.evaluateAll((cards) => cards.every((card) => {
    const cardRect = card.parentElement!.getBoundingClientRect();
    const rankRect = card.firstElementChild!.getBoundingClientRect();
    const copyRect = card.lastElementChild!.getBoundingClientRect();
    return rankRect.left - cardRect.left >= 12 && cardRect.right - copyRect.right >= 12;
  }))).toBe(true);

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
  const damageSplit = firstRankedRow.getByRole("list", { name: "Attack rating split" });
  for (const label of [
    "Physical attack rating: 700",
    "Magic attack rating: 0",
    "Fire attack rating: 0",
    "Lightning attack rating: 0",
    "Holy attack rating: 0",
  ]) {
    await expect(damageSplit.getByRole("listitem", { name: label })).toBeVisible();
  }
  await expect.poll(() => page.getByRole("columnheader", { name: "Weapon" }).evaluate(
    (node) => getComputedStyle(node).textAlign,
  )).toBe("center");
  await expect(page.getByRole("columnheader", { name: "AR / Elements / Status" })).toBeVisible();
  await page.getByText("Model coverage and assumptions").click();
  await expect(page.getByText("Attack rating is calculated before enemy defense and negation.")).toBeVisible();
  await expect(page.getByText(/Temporary buff stacking is not a universal layer/)).toBeVisible();
  await expectRankingsBoardToFit(page);
  await expect(page.getByRole("button", { name: "Show lowest rank first" })).toBeVisible();
  await page.getByRole("button", { name: "Show lowest rank first" }).click();
  await expect(page.locator(".result-row-full").first().locator(".rank-cell")).toHaveText("4");
  await page.getByRole("button", { name: "Show best rank first" }).click();
  await expect(page.locator(".result-row-full").first().locator(".rank-cell")).toHaveText("1");
  await expect.poll(() => page.locator(".result-row-full").first().locator("[role=gridcell]").nth(2).evaluate(
    (node) => getComputedStyle(node).position,
  )).toBe("sticky");
  await page.getByRole("button", { name: "Select Uchigatana, Occult, rank 2" }).click();
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
  await expect.poll(async () => {
    const widths = await page.locator(".result-row-full .damage-token-grid .metric-token, .result-row-full .status-token-grid .metric-token")
      .evaluateAll((nodes) => nodes.map((node) => Math.round(node.getBoundingClientRect().width * 10) / 10));
    return widths.length > 0 && new Set(widths).size === 1 && Math.min(...widths) > 0;
  }).toBe(true);

  await page.locator(".top-card").first().getByRole("button", { name: "Lock" }).click();
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
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("Selected").first()).toBeVisible();
  await expect(page.getByText("Compare").first()).toBeVisible();
  await expect(page.locator(".path-chart")).toContainText("Stat breakpoint");
  await expect.poll(() => page.locator(".path-chart .spark-line").first().evaluate(
    (node) => node.scrollWidth <= node.clientWidth + 1,
  )).toBe(true);
  await expect.poll(() => page.evaluate(
    () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
  )).toBe(true);
  await expect.poll(() => page.locator(".paths-panel").evaluate((node) => getComputedStyle(node).overflowY)).toBe("auto");

  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  const affinityRankings = page.getByRole("grid", { name: "Affinity watch rankings" });
  await expect(affinityRankings).toContainText("Keen");
  await expect(page.locator(".affinity-chart")).toContainText("Best-affinity crossover");
  await expect(page.locator(".affinity-plot svg")).toBeVisible();
  await expect(page.locator(".affinity-chart .spark-line")).toHaveCount(0);
  await expect(affinityRankings).toContainText("Occult");
});

test("somber-only exact search uses the somber upgrade cap", async ({ page }) => {
  await page.goto("/");

  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("60");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).press("Enter");
  await expect(page.getByRole("textbox", { name: "Level", exact: true })).toHaveValue("57");
  await expect(page.getByRole("spinbutton", { name: "Standard Upgrade" })).toHaveValue("25");
  await expect(page.getByRole("spinbutton", { name: "Somber Upgrade" })).toHaveValue("10");
  await page.getByRole("button", { name: "Use exact levels" }).click();
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
      && Math.max(...boxes.map((box) => box.top)) - Math.min(...boxes.map((box) => box.top)) < 2
      && Math.max(...boxes.map((box) => box.height)) - Math.min(...boxes.map((box) => box.height)) < 2;
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
  await keen.getByRole("button", { name: "Compare", exact: true }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(page.getByText("Selected baseline versus 1 pinned target")).toBeVisible();
  await expect(page.locator(".compare-lane", { hasText: "Pinned #1" })).toContainText("Keen");

  await page.getByRole("button", { name: "Clear 1 pinned target" }).click();
  await expect(page.locator(".compare-lane", { hasText: "Selected" })).toContainText("Occult");
  await expect(page.getByText("Selected baseline versus current ranked rivals")).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "Rankings" }).click();
  await occult.getByRole("button", { name: "Compare", exact: true }).click();
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
  await keen.getByRole("button", { name: "Compare", exact: true }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(page.getByRole("button", { name: "Compare Type" })).toContainText("All");
  await expect(page.getByText("Selected baseline versus 1 pinned target")).toBeVisible();
  await expect(page.locator(".compare-lane", { hasText: "Pinned #1" })).toContainText("Keen");
  await expect(page.locator(".compare-lane", { hasText: "Selected" })).toContainText("Occult");
});

test("a ranked row below the podium selects that exact build", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  const fourthRow = page.locator(".result-row-full").nth(3);
  const weaponName = (await fourthRow.locator(".weapon-cell strong").textContent())?.trim();
  expect(weaponName).toBeTruthy();

  await fourthRow.click();
  await expect(fourthRow).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".selected-build strong")).toHaveText(weaponName ?? "");

  const thirdRow = page.locator(".result-row-full").nth(2);
  await thirdRow.focus();
  await thirdRow.press("Enter");
  await expect(thirdRow).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".selected-build")).toContainText("Keen / Seppuku / +25");
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

  const animationDurationMs = await page.locator(".ambient-field i").first().evaluate(
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
