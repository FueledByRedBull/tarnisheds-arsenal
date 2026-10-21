import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { chooseClass, openEditor, openSection, type EditorName } from "./editors";

// WCAG 2.2 A/AA rules over each surface a player reaches: the strip, its editors, the
// palette, results with Build Detail, Compare, Paths, Affinity Watch and Saved Builds.
async function violations(page: Page, include?: string) {
  const builder = new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]);
  const { violations, incomplete } = await (include ? builder.include(include) : builder).analyze();
  // Rules axe could not decide (contrast over gradients, overlays) need a human look.
  for (const result of incomplete) {
    test.info().annotations.push({ type: "axe-incomplete", description: `${result.id}: ${result.nodes.map((node) => node.target.join(" ")).join(", ")}` });
  }
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
  const classPanel = await openEditor(page, "Class");
  await classPanel.getByRole("combobox", { name: "Class" }).press("ArrowDown");
  await expect(classPanel.getByRole("listbox", { name: "Class" })).toBeVisible();
  expect(await violations(page, ".popover-panel:popover-open"), "open Class list").toEqual([]);
  await page.keyboard.press("Escape");
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
  // Markers fade in from the frame after the new rows commit; judge their settled contrast.
  await page.locator(".rank-move").evaluateAll(async (markers) => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    await Promise.all(markers.flatMap((marker) => marker.getAnimations()).map((animation) => animation.finished));
  });
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

test("trade-offs, Paths, Affinity Watch, Saved Builds, the problem report and errors have no WCAG A/AA violations", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  // The finished search's progress line fades out; mid-fade it is low contrast by design.
  await expect(page.locator(".progress-meta")).toHaveCount(0);
  const navigation = page.getByRole("navigation");

  await navigation.getByRole("button", { name: "Compare", exact: true }).click();
  const tradeoffs = page.getByRole("region", { name: "AR / Bleed tradeoffs" });
  await tradeoffs.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(tradeoffs.getByRole("table", { name: "Trade-off options", exact: true })).toBeVisible();
  expect(await violations(page), "Compare with trade-offs").toEqual([]);

  await navigation.getByRole("button", { name: "Paths", exact: true }).click();
  await page.getByRole("button", { name: "Trace paths", exact: true }).click();
  await expect(page.getByRole("table", { name: "Path steps", exact: true }).getByRole("row").nth(1)).toBeVisible();
  expect(await violations(page), "Paths").toEqual([]);

  await navigation.getByRole("button", { name: "Affinity Watch", exact: true }).click();
  await page.getByRole("button", { name: "Watch affinities", exact: true }).click();
  await expect(page.getByRole("table", { name: "Affinity watch rankings" }).getByRole("row").nth(1)).toBeVisible();
  expect(await violations(page), "Affinity Watch").toEqual([]);

  await navigation.getByRole("button", { name: "Rankings", exact: true }).click();
  await openSection(page, "Saved Builds");
  await page.getByText("Backup and recovery", { exact: true }).click();
  await openSection(page, "Report a problem");
  expect(await violations(page), "Saved Builds, backup and problem report").toEqual([]);

  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().setError("Search failed: the native worker stopped.");
  });
  await expect(page.locator('.error-strip[role="alert"]')).toBeVisible();
  expect(await violations(page, ".error-strip"), "error strip").toEqual([]);
});

test("the fixed-stats readout does not name Convergence while Vanilla is shown", async ({ page }) => {
  // A session that opens on Convergence has no Vanilla query to restore on the switch.
  await page.addInitScript(() => localStorage.setItem("tarnisheds-arsenal.gameProfile.v1", "convergence"));
  await page.goto("/");
  const profiles = page.getByRole("radiogroup", { name: "Game profile" });
  await expect(page.getByRole("button", { name: "Class: Retainer" })).toBeVisible();
  await chooseClass(page, "Custom stats");
  // Vanilla's catalog is held, as a slow or failed load leaves it, so the carried class stays.
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    api.catalog = () => new Promise(() => {});
  });
  await profiles.getByRole("radio", { name: /Vanilla/ }).click();
  await expect(profiles.getByRole("radio", { name: /Vanilla/ })).toHaveAttribute("aria-checked", "true");
  const readout = page.locator(".ribbon-readout").filter({ hasText: "Fixed stats" });
  await expect(readout).toContainText("entered combat stats exactly");
  await expect(readout).not.toContainText("Convergence");
  await expect(readout).not.toHaveAttribute("title", /Convergence/);
});
