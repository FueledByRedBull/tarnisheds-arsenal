import { expect, test } from "@playwright/test";

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
  await expectRankingsBoardToFit(page);
  await page.getByRole("button", { name: "Select Uchigatana, Occult, rank 2" }).click();
  await expect(page.locator(".selected-build")).toContainText("Occult / Seppuku / +25");

  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("13");
  await expect(page.getByText("0 ranked rows")).toBeVisible();
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  await page.locator(".top-card").first().getByRole("button", { name: "Lock" }).click();
  await expect(page.getByText("Exact upgrade and stat locks active")).toBeVisible();
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

  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("Keen").first()).toBeVisible();
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

async function chooseSearchableOption(page: import("@playwright/test").Page, label: string, option: string) {
  const field = page.getByRole("combobox", { name: label, exact: true });
  await field.fill(option);
  await page.keyboard.press("Enter");
}

async function expectRankingsBoardToFit(page: import("@playwright/test").Page) {
  const board = page.getByRole("grid", { name: "Ranked builds" });
  await expect.poll(() => board.evaluate((node) => node.scrollWidth <= node.clientWidth + 2)).toBe(true);
}
