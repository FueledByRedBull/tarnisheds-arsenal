import { expect, test } from "@playwright/test";

test("session-driven search, lock, compare, paths, and affinity watch", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByRole("textbox", { name: "Level" })).toHaveValue("9");
  await expect(page.getByText("Redistrib", { exact: true })).toBeVisible();
  await page.getByRole("combobox", { name: "Class" }).click();
  await expect(page.getByRole("option", { name: "Wretch" })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("spinbutton", { name: "STR" }).fill("130");
  await expect(page.getByRole("spinbutton", { name: "STR" })).toHaveValue("130");
  await page.getByRole("spinbutton", { name: "STR" }).press("Enter");
  await expect(page.getByRole("spinbutton", { name: "STR" })).toHaveValue("99");
  await page.getByRole("combobox", { name: "Class" }).click();
  await page.getByRole("option", { name: "Vagabond" }).click();
  await expect(page.getByRole("spinbutton", { name: "VIG" })).toHaveValue("15");
  await expect(page.getByRole("spinbutton", { name: "STR" })).toHaveValue("14");
  await expect(page.getByRole("spinbutton", { name: "DEX" })).toHaveValue("13");
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
  await expectRankingsBoardToDragScroll(page);

  await page.getByRole("spinbutton", { name: "STR" }).fill("13");
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

  await page.getByRole("spinbutton", { name: "STR" }).fill("60");
  await page.getByRole("spinbutton", { name: "STR" }).press("Enter");
  await expect(page.getByRole("textbox", { name: "Level" })).toHaveValue("57");
  await expect(page.getByRole("spinbutton", { name: "Standard Upgrade" })).toHaveValue("25");
  await expect(page.getByRole("spinbutton", { name: "Somber Upgrade" })).toHaveValue("10");
  await page.getByRole("checkbox", { name: "Exact" }).check();
  await page.getByRole("button", { name: "Advanced Show" }).click();
  await chooseSearchableOption(page, "Somber", "Somber Only");

  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("1 ranked row")).toBeVisible();
  await expect(page.getByRole("grid", { name: "Ranked builds" })).toContainText("Somber");
  await expect(page.getByRole("grid", { name: "Ranked builds" })).toContainText("Ancient Meteoric Ore Greatsword");
  await expect(page.getByRole("grid", { name: "Ranked builds" })).toContainText("+10");
});

async function chooseSearchableOption(page: import("@playwright/test").Page, label: string, option: string) {
  const field = page.getByRole("combobox", { name: label, exact: true });
  await field.fill(option);
  await page.keyboard.press("Enter");
}

async function expectRankingsBoardToDragScroll(page: import("@playwright/test").Page) {
  const board = page.getByRole("grid", { name: "Ranked builds" });
  await expect.poll(() => board.evaluate((node) => node.scrollWidth > node.clientWidth)).toBe(true);

  const box = await board.boundingBox();
  expect(box).not.toBeNull();
  if (!box) return;

  const before = await board.evaluate((node) => node.scrollLeft);
  await page.mouse.move(box.x + box.width - 120, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 80, box.y + box.height / 2, { steps: 8 });
  await page.mouse.up();

  await expect.poll(() => board.evaluate((node) => node.scrollLeft)).toBeGreaterThan(before + 100);

  const rowBounds = await board.locator(".result-row-full").first().evaluate((row) => {
    const rowRect = row.getBoundingClientRect();
    const lastCellRect = row.lastElementChild?.getBoundingClientRect();
    return {
      lastCellRight: lastCellRect?.right ?? 0,
      rowRight: rowRect.right,
    };
  });
  expect(rowBounds.rowRight).toBeGreaterThanOrEqual(rowBounds.lastCellRight);
}
