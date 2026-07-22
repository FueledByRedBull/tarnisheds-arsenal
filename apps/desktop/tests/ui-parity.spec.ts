import { expect, test } from "@playwright/test";

test("profile switch isolates results and explains Convergence coverage", async ({ page }) => {
  await page.goto("/");
  const profiles = page.getByRole("radiogroup", { name: "Game profile" });
  await expect(profiles.getByRole("radio", { name: /Vanilla/ })).toHaveAttribute("aria-checked", "true");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  await profiles.getByRole("radio", { name: /Convergence/ }).click();
  await expect(profiles.getByRole("radio", { name: /Convergence/ })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText("Weapon model ready", { exact: true })).toBeVisible();
  await expect(
    page.getByText(/Ammo weapons and AoW hit\/route damage stay disabled/),
  ).toBeVisible();
  await expect(page.getByText("4 ranked rows")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "AoW First Hit" })).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => localStorage.getItem("tarnisheds-arsenal.gameProfile.v1"))).toBe("convergence");
});

test("session-driven search, lock, compare, paths, and affinity watch", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/");

  await expect(page.getByRole("textbox", { name: "Level" })).toHaveValue("9");
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
  await expect(page.getByRole("textbox", { name: "Level" })).toHaveValue("9");
  await chooseSearchableOption(page, "Weapon Type", "Great Katana");
  await expect(page.getByRole("combobox", { name: "Weapon Type" })).toHaveValue("Great Katana");
  await chooseSearchableOption(page, "Weapon Type", "Open");
  await chooseSearchableOption(page, "Weapon", "Zweihander");
  await expect(page.getByRole("combobox", { name: "Weapon", exact: true })).toHaveValue("Zweihander");
  await chooseSearchableOption(page, "Weapon", "Open");
  await chooseSearchableOption(page, "AoW", "Bloodhound's Step");
  await expect(page.getByRole("combobox", { name: "AoW" })).toHaveValue("Bloodhound's Step");
  await chooseSearchableOption(page, "AoW", "Open");

  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await expect(page.getByText("Uchigatana").first()).toBeVisible();
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

  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("13");
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await expect(page.getByText("Inputs changed")).toBeVisible();
  await page.getByRole("button", { name: "Update Results" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  await page.locator(".top-card").first().getByRole("button", { name: "Lock" }).click();
  await expect(page.getByText("Exact upgrade and stat locks active")).toBeVisible();
  await expect(page.locator(".active-lock-warning")).toContainText("Changing class or loadout keeps these locks");
  await expect(page.getByText("Blood / Seppuku / +25").first()).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await expect(page.getByText("Selected line, explicit target, or top ranked rivals")).toBeVisible();
  await expect(page.locator(".compare-lane", { hasText: "Selected" })).toContainText("Uchigatana");
  await expect(page.locator(".compare-lane", { hasText: "Selected" })).toContainText("Blood / Seppuku");
  await expect(page.locator(".scaling-strip").first()).toContainText("STR C (0.61)");
  await expect(page.locator(".scaling-strip").first()).toContainText("DEX B (0.93)");
  await expect(page.locator(".scaling-strip").first()).toContainText("ARC D (0.44)");
  await chooseSearchableOption(page, "Compare Weapon", "Uchigatana");
  await chooseSearchableOption(page, "Compare Affinity", "Occult");
  await expect(page.locator(".compare-lane", { hasText: "Target" })).toContainText("Occult / Seppuku");
  await expect(page.locator(".compare-lane", { hasText: "Target" })).toContainText("ARC 60");
  await expect(page.locator(".compare-lane", { hasText: "Target" })).toContainText("AR 670");
  await expect(page.locator(".compare-lane", { hasText: "Target" })).toContainText("STR E (0.21)");
  await expect(page.locator(".compare-lane", { hasText: "Target" })).toContainText("DEX D (0.33)");
  await expect(page.locator(".compare-lane", { hasText: "Target" })).toContainText("ARC B (1.39)");
  await page.locator(".matrix-toolbar").getByRole("button", { name: "+25" }).click();
  await expect(page.locator(".metric-matrix").getByText("+25")).toBeVisible();
  await expect(page.getByText("+25").first()).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "Paths" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("Selected").first()).toBeVisible();
  await expect(page.getByText("Compare").first()).toBeVisible();
  await expect(page.locator(".path-chart")).toContainText("Stat breakpoint");
  await expect.poll(() => page.locator(".paths-panel").evaluate((node) => getComputedStyle(node).overflowY)).toBe("auto");

  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("Keen").first()).toBeVisible();
  await expect(page.locator(".affinity-chart")).toContainText("Best-affinity crossover");
  await expect(page.getByRole("grid", { name: "Affinity watch rankings" })).toContainText("Occult");
});

test("somber-only exact search uses the somber upgrade cap", async ({ page }) => {
  await page.goto("/");

  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("60");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).press("Enter");
  await expect(page.getByRole("textbox", { name: "Level" })).toHaveValue("57");
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

  await page.evaluate(() => {
    const presetKey = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v1."));
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

async function expectRankingsBoardToFit(page: import("@playwright/test").Page) {
  const board = page.getByRole("grid", { name: "Ranked builds" });
  await expect.poll(() => board.evaluate((node) => node.scrollWidth <= node.clientWidth + 2)).toBe(true);
}
