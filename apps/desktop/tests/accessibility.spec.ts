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
  for (const editor of ["Class", "Objective", "Loadout", "Upgrades", "Limits"] satisfies EditorName[]) {
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
  // A finished search's progress strip fades out; check the page once it has gone.
  await expect(page.locator(".progress-strip")).toHaveCount(0);
  await page.locator(".result-row").nth(1).click();
  expect(await violations(page)).toEqual([]);
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  const filters = await openEditor(page, "Comparison filters");
  await filters.getByRole("button", { name: "Compare Type", exact: true }).click();
  await filters.getByRole("group", { name: "Compare Type", exact: true }).getByRole("checkbox", { name: /^Katana\b/ }).click();
  await page.keyboard.press("Escape");
  expect(await violations(page)).toEqual([]);
});

test("movement markers, sorted headers and the shortcut sheet have no WCAG A/AA violations", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const [first, second, third, fourth] = useDesktopStore.getState().rows;
    useDesktopStore.getState().setRows([
      { ...second, ar: { ...second.ar, total: second.ar.total + 40 } },
      { ...first, ar: { ...first.ar, total: first.ar.total - 20 } },
      third,
      { ...fourth, aowName: "Different skill" },
    ], 100);
  });
  // The selected row tints its background, so check the down marker on it too.
  await page.locator(".result-row").nth(1).click();
  await page.getByRole("columnheader", { name: "AR", exact: true }).getByRole("button").click();
  await expect(page.locator(".rank-move")).toHaveCount(3);
  expect(await violations(page, ".rankings-panel")).toEqual([]);
  expect(await violations(page, ".inspector")).toEqual([]);
  await page.keyboard.press("?");
  await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
  expect(await violations(page, ".shortcuts-dialog")).toEqual([]);
});

test("the rankings grid is one tab stop and the Compare tab reads its pinned count", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  const rows = page.locator(".result-row");
  await expect(page.locator('.result-row[tabindex="0"]')).toHaveCount(1);
  await expect(rows.first()).toHaveAttribute("tabindex", "0");
  await rows.nth(2).click();
  await expect(rows.nth(2)).toHaveAttribute("tabindex", "0");
  await expect(page.locator('.result-row[tabindex="0"]')).toHaveCount(1);
  // Arrows still move between rows, and Tab then leaves the grid in one press.
  await rows.nth(2).focus();
  await page.keyboard.press("ArrowDown");
  await expect(rows.nth(3)).toBeFocused();
  await page.keyboard.press("p");
  const compare = page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true });
  await expect(compare).toHaveAccessibleDescription(/1 pinned/);
});

test("the disclosed controls (column guide, export, How it works, More details) have no WCAG A/AA violations", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await expect(page.locator(".progress-strip")).toHaveCount(0);
  await page.locator(".result-row").nth(1).click();
  await page.getByRole("button", { name: "Column guide", exact: true }).click();
  expect(await violations(page, ".popover-panel:popover-open")).toEqual([]);
  await page.keyboard.press("Escape");
  // Keyboard reaches the export choices, and Escape returns focus to the trigger.
  await page.getByRole("button", { name: "Export CSV", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("group", { name: "Rows to export" })).toBeVisible();
  expect(await violations(page, ".popover-panel:popover-open")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "Export CSV", exact: true })).toBeFocused();
  await page.locator("summary").filter({ hasText: "More details" }).click();
  expect(await violations(page, ".inspector")).toEqual([]);
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await page.getByRole("button", { name: "About Compare", exact: true }).click();
  expect(await violations(page, ".popover-panel:popover-open")).toEqual([]);
  await page.keyboard.press("Escape");
  expect(await violations(page, ".compare-panel")).toEqual([]);
});
