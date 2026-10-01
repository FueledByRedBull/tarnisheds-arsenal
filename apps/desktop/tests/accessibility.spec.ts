import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { openEditor, type EditorName } from "./editors";

// WCAG 2.1 A/AA rules over each surface a player reaches: the strip, its editors, the
// palette, results with Build Detail, and Compare with its filters open.
async function violations(page: Page, include?: string) {
  const builder = new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]);
  const { violations } = await (include ? builder.include(include) : builder).analyze();
  return violations.map((violation) => ({
    id: violation.id,
    impact: violation.impact,
    targets: violation.nodes.map((node) => node.target.join(" ")),
  }));
}

test("the strip, every editor and the palette have no WCAG A/AA violations", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  expect(await violations(page)).toEqual([]);
  for (const editor of ["Class", "Objective", "Loadout", "Upgrades", "Scaling", "Results", "Limits"] satisfies EditorName[]) {
    await openEditor(page, editor);
    expect(await violations(page, ".popover-panel:popover-open"), editor).toEqual([]);
  }
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "Command" }).fill("str 4");
  expect(await violations(page, ".command-palette")).toEqual([]);
});

test("results, Build Detail and Compare filters have no WCAG A/AA violations", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.locator(".result-row-full").nth(1).click();
  expect(await violations(page)).toEqual([]);
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  const filters = await openEditor(page, "Comparison filters");
  await filters.getByRole("button", { name: "Compare Type", exact: true }).click();
  await filters.getByRole("group", { name: "Compare Type", exact: true }).getByRole("checkbox", { name: /^Katana\b/ }).click();
  await page.keyboard.press("Escape");
  expect(await violations(page)).toEqual([]);
});
