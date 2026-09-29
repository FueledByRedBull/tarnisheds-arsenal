import { expect, test } from "@playwright/test";

test("missing skill routes stay unavailable across displays, reports, and saved-build reload", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const state = useDesktopStore.getState();
    const [missing, modeled] = state.rows;
    state.setRows([
      { ...missing, aowRoute: null, aowFirstHitDamage: 0, aowFullSequenceDamage: 0 },
      { ...modeled, aowFirstHitDamage: 0, aowFullSequenceDamage: 0,
        aowRoute: { routeId: "zero", routeLabel: "Zero damage", routePriority: 0,
          buffActivationActionId: null, actions: [], firstHitDamage: 0,
          totalDamage: { physical: 0, magic: 0, fire: 0, lightning: 0, holy: 0, total: 0 },
          totalPoiseDamage: 0, totalStaminaCost: 0,
          totalStatusBuildup: { bleed: 0, frost: 0, poison: 0, scarletRot: 0, sleep: 0, madness: 0, death: 0 } } },
    ]);
  });
  const resultRows = page.locator(".result-row-full");
  await expect(resultRows.first().getByRole("gridcell").nth(5)).toHaveText("Unavailable");
  await expect(resultRows.nth(1).getByRole("gridcell").nth(5)).toHaveText("0First 0");
  await expect(page.locator(".inspector .metric-grid")).toContainText("Raw AoWUnavailable");
  await page.getByRole("button", { name: "Preview reproduction report", exact: true }).click();
  const report = JSON.parse(await page.getByRole("textbox", { name: "Reproduction report preview" }).inputValue());
  expect(report.results.selected.aowFirstHitDamage).toBeNull();
  expect(report.results.selected.aowFullSequenceDamage).toBeNull();
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  await expect(page.getByRole("group", { name: "Selected baseline", exact: true })).toContainText("AoW Unavailable");
  await expect(page.getByRole("group", { name: "Top #2", exact: true })).toContainText("AoW 0");
  await expect(page.getByRole("table", { name: "Primary deltas versus baseline" }).getByRole("cell").last()).toHaveText("Unavailable");
  await page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true }).click();
  await resultRows.nth(1).click();
  await page.getByRole("button", { name: "Preview reproduction report", exact: true }).click();
  const zeroReport = JSON.parse(await page.getByRole("textbox", { name: "Reproduction report preview" }).inputValue());
  expect(zeroReport.results.selected.aowFirstHitDamage).toBe(0);
  expect(zeroReport.results.selected.aowFullSequenceDamage).toBe(0);
  await expect(page.locator(".inspector .metric-grid")).toContainText("Raw AoW0");
  await page.reload();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.locator(".inspector .metric-grid")).toContainText("Raw AoWUnavailable");
});
