import { expect, test, type Page } from "@playwright/test";
import { violations } from "./axe";
import { chooseClass, closeEditors, openEditor, openSection } from "./editors";

async function ready(page: Page) {
  await page.goto("/");
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
}

async function openConvergence(page: Page) {
  await page.goto("/");
  await page.getByRole("radiogroup", { name: "Game profile" }).getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental model", { exact: true })).toBeVisible();
}

async function search(page: Page) {
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await expect(page.locator(".progress-strip")).toHaveCount(0);
}

test("Vanilla marks Somber weapons; Convergence has no Somber split", async ({ page }) => {
  await ready(page);
  await search(page);
  await expect(page.locator(".loadout-tag").first()).toHaveText("Somber");
  await page.locator(".result-row").first().click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  const filters = await openEditor(page, "Comparison filters");
  await expect(filters.getByRole("group", { name: "Compare Reinforcement" })).toBeVisible();
  await closeEditors(page);

  await openConvergence(page);
  await search(page);
  await expect(page.locator(".loadout-tag")).toHaveCount(0);
  await page.locator(".result-row").first().click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  const convergenceFilters = await openEditor(page, "Comparison filters");
  await expect(convergenceFilters.getByRole("group", { name: "Compare Reinforcement" })).toHaveCount(0);
  await expect(convergenceFilters.getByText("Smithing", { exact: true })).toHaveCount(0);
  await expect(convergenceFilters.getByText("Somber", { exact: true })).toHaveCount(0);
  await closeEditors(page);
  // A stored toggle never becomes a hidden chip or a hidden filter there.
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().patchCompareControls({ includeSomber: false });
  });
  await expect(page.getByRole("list", { name: "Active comparison filters" })).toHaveCount(0);
  await expect(page.getByText("Smithing only")).toHaveCount(0);
});

test("Convergence panels have no WCAG A/AA violations", async ({ page }) => {
  await openConvergence(page);
  await chooseClass(page, "Custom stats");
  await openEditor(page, "Class");
  expect(await violations(page, ".popover-panel:popover-open"), "Custom stats class editor").toEqual([]);
  await closeEditors(page);
  await openEditor(page, "Limits");
  const options = page.getByRole("dialog", { name: "More options", exact: true });
  await expect(options.getByRole("checkbox", { name: "Use entered combat stats exactly" })).toBeDisabled();
  expect(await violations(page, ".popover-panel:popover-open"), "limits in fixed-stats mode").toEqual([]);
  await closeEditors(page);
  await openEditor(page, "Upgrades");
  expect(await violations(page, ".popover-panel:popover-open"), "upgrade editor").toEqual([]);
  await closeEditors(page);
  await page.getByRole("button", { name: /^Experimental model/ }).click();
  await expect(page.getByRole("dialog", { name: "Model coverage and assumptions" })).toBeVisible();
  expect(await violations(page, ".popover-panel:popover-open"), "coverage popover").toEqual([]);
  await closeEditors(page);

  await chooseClass(page, "Retainer");
  await search(page);
  await page.locator(".result-row").first().click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  await openEditor(page, "Comparison filters");
  expect(await violations(page, ".popover-panel:popover-open"), "Compare filters").toEqual([]);
  await closeEditors(page);
  expect(await violations(page, ".compare-panel"), "Compare").toEqual([]);
});

test("unavailable controls say why, in text a screen reader reads with them", async ({ page }) => {
  await ready(page);
  await openSection(page, "Saved Builds");
  await expect(page.locator("#saved-builds-hint")).toContainText("No saved builds yet");
  for (const name of ["Update selected", "Load", "Rename", "Delete", "Export", "Copy share text"]) {
    const button = page.getByRole("button", { name, exact: true });
    await expect(button).toBeDisabled();
    await expect(button).toHaveAccessibleDescription(/No saved builds yet/);
  }
  await search(page);
  const exportButton = page.getByRole("button", { name: "Export CSV", exact: true });
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState({ isSearching: true });
  });
  await expect(exportButton).toBeDisabled();
  await expect(exportButton).toHaveAccessibleDescription(/available when the search finishes/);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState({ isSearching: false, isExporting: true });
  });
  // A search asked for during an export waits for it instead of being dropped.
  const searchButton = page.getByRole("button", { name: "Search", exact: true });
  await expect(searchButton).toBeEnabled();
  await expect(searchButton).toHaveAccessibleDescription("A search started now runs when the CSV export finishes.");
  await searchButton.click();
  await expect(searchButton).toHaveAccessibleDescription("Search queued. It runs when the CSV export finishes.");
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState({ isExporting: false });
  });
  await expect(page.getByText("4 ranked rows")).toBeVisible();
});

test("stat-lock controls explain the empty lock list and Custom stats mode", async ({ page }) => {
  await openConvergence(page);
  await openEditor(page, "Limits");
  const options = page.getByRole("dialog", { name: "More options", exact: true });
  await expect(options.getByRole("button", { name: "Clear stat locks" })).toHaveAccessibleDescription(/None: every stat is free/);
  await closeEditors(page);
  await chooseClass(page, "Custom stats");
  await openEditor(page, "Limits");
  await expect(options.getByRole("checkbox", { name: "Use entered combat stats exactly" })).toHaveAccessibleDescription(/exactly/);
  await expect(options.getByRole("button", { name: "Clear stat locks" })).toHaveAccessibleDescription(/exactly/);
});

test("a stat typed past its range is marked, limited and announced", async ({ page }) => {
  await ready(page);
  const str = page.getByRole("spinbutton", { name: "STR", exact: true });
  await str.fill("150");
  await expect(str).toHaveAttribute("aria-invalid", "true");
  await expect(str).toHaveAccessibleDescription(/Allowed 12 to 99/);
  await str.press("Enter");
  await expect(str).toHaveValue("99");
  await expect(str).not.toHaveAttribute("aria-invalid", "true");
  await expect(page.locator("#app-announcer")).toHaveText("Limited to 99 (allowed 12-99)");
  // The announcement is never part of the field's name.
  await expect(str).toHaveAccessibleName("STR");
  await str.fill("1");
  await str.press("Tab");
  await expect(str).toHaveValue("12");
  await expect(page.locator("#app-announcer")).toHaveText("Limited to 12 (allowed 12-99)");
});

test("Levels ahead states its range and limits a longer horizon", async ({ page }) => {
  await ready(page);
  await search(page);
  await page.locator(".result-row").first().click();
  await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
  const levels = page.locator(".paths-panel").getByRole("spinbutton", { name: "Levels ahead" });
  await expect(levels).toHaveAccessibleDescription(/Allowed 1 to 200/);
  await levels.fill("500");
  await levels.press("Enter");
  await expect(levels).toHaveValue("200");
  await expect(page.locator("#app-announcer")).toHaveText("Limited to 200 (allowed 1-200)");
});

test("the Convergence upgrade editor ties its note to the input", async ({ page }) => {
  await openConvergence(page);
  const upgrades = await openEditor(page, "Upgrades");
  await expect(upgrades.getByRole("spinbutton", { name: "Weapon Upgrade", exact: true }))
    .toHaveAccessibleDescription(/one \+0 to \+15 reinforcement path/);
});

test("the blessing editor disables its level until DLC scaling is on and reads in plain language", async ({ page }) => {
  await ready(page);
  await openEditor(page, "Scaling");
  const options = page.getByRole("dialog", { name: "More options", exact: true });
  const level = options.getByRole("spinbutton", { name: "Scadutree Level" });
  await expect(level).toBeDisabled();
  await expect(level).toHaveAccessibleDescription(/Turn on DLC Scaling/);
  await options.getByRole("checkbox", { name: "DLC Scaling" }).check();
  await expect(level).toBeEnabled();
  await level.fill("10");
  await level.press("Enter");
  await expect(options.getByText(/^Deal 1\.\d\dx damage, take 0\.\d{3}x$/)).toBeVisible();
  await expect(options.getByText(/% less damage taken$/)).toBeVisible();
});

test("Build Detail is a named heading and landmark", async ({ page }) => {
  await ready(page);
  await expect(page.getByRole("heading", { level: 2, name: "Build detail" })).toBeVisible();
  await expect(page.getByRole("complementary", { name: "Build detail" })).toBeVisible();
});

test("Affinity Watch lines differ by dash pattern, not colour alone", async ({ page }) => {
  await ready(page);
  await search(page);
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const line = (affinity: string, base: number) => ({
      affinity, points: [{ level: 10, metric: base }, { level: 11, metric: base + 5 }],
      startMetric: base, endMetric: base + 5, finalBuild: null,
    });
    api.startAffinityWatch = async () => ({ jobId: "dash-affinity" });
    api.affinityWatchStatus = async (jobId) => ({
      progress: null,
      finished: { jobId, cancelled: false, error: null, payload: { lines: [line("A", 100), line("B", 200), line("C", 300)], breakpoints: [] } },
    });
  });
  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch", exact: true }).click();
  await page.getByRole("button", { name: "Watch affinities", exact: true }).click();
  await expect(page.locator(".analysis-progress:visible")).toHaveAttribute("data-analysis-status", "completed");
  const dashes = await page.locator(".affinity-series").evaluateAll((lines) => lines.map((line) => line.getAttribute("stroke-dasharray")));
  expect(dashes).toEqual([null, "14 6", "6 6"]);
  const swatches = await page.locator(".affinity-swatch line").evaluateAll((lines) => lines.map((line) => line.getAttribute("stroke-dasharray")));
  expect(swatches).toEqual(dashes);
});

test("a search that finds nothing names the constraints to relax", async ({ page }) => {
  await ready(page);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().patchRequest({ weaponName: "Uchigatana", somberFilter: "somber_only", exactUpgrade: true });
  });
  await page.getByRole("button", { name: "Search", exact: true }).click();
  const empty = page.locator(".result-board .empty-state");
  await expect(empty).toContainText("No legal setup fits this query");
  await expect(empty).toContainText("the Uchigatana weapon");
  await expect(empty).toContainText("the reinforcement filter");
  await expect(empty).toContainText("the exact upgrade level");
  await expect(page.getByText("Find the best weapon for this character")).toHaveCount(0);
});

test("a gate tells the player the next step", async ({ page }) => {
  await ready(page);
  await search(page);
  await page.locator(".result-row").first().click();
  await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState({ activeWorkspace: "compare" });
  });
  await expect(page.getByText("Update Rankings before comparing")).toBeVisible();
  await expect(page.getByText("Go to Rankings, then press Run updated search.")).toBeVisible();
});

test("small controls carry descriptive names", async ({ page }) => {
  await ready(page);
  await search(page);
  await page.locator(".result-row").first().click();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  await openSection(page, "Upgrade matrix");
  await expect(page.getByRole("button", { name: "Scroll upgrade matrix to +0" })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Scroll upgrade matrix to \+\d+$/ })).toHaveCount(2);
  // The sacrifice field flags and explains an entry it cannot use.
  const sacrifice = page.getByRole("spinbutton", { name: "Max AR sacrifice (%)" });
  await sacrifice.fill("101");
  await expect(sacrifice).toHaveAttribute("aria-invalid", "true");
  await expect(sacrifice).toHaveAccessibleDescription(/Enter 0 to 100/);
});

test("the palette announces how many commands match", async ({ page }) => {
  await ready(page);
  await page.keyboard.press("Control+k");
  const input = page.getByRole("combobox", { name: "Command" });
  await input.fill("str 4");
  await expect(page.locator(".command-palette p[role=status]")).toHaveText(/^\d+ commands? available$/);
  await input.fill("zzzzzz");
  await expect(page.locator(".command-palette p[role=status]")).toContainText("No command matches");
});

test("Update selected cannot rename the first saved build to the ranked build's name on first open", async ({ page }) => {
  await ready(page);
  await openSection(page, "Saved Builds");
  await page.getByLabel("Name", { exact: true }).fill("Alpha plan");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.reload();
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await search(page);
  await page.locator(".result-row").first().click();
  await openSection(page, "Saved Builds");
  await expect(page.getByRole("combobox", { name: "Saved", exact: true }).locator("option:checked")).toContainText("Alpha plan");
  await expect(page.getByLabel("Name", { exact: true })).toHaveValue("Alpha plan");
  await page.getByRole("button", { name: "Update selected", exact: true }).click();
  expect(await page.evaluate(() => (JSON.parse(localStorage.getItem("tarnisheds-arsenal.savedBuildIndex.v1")!) as { builds: { name: string }[] })
    .builds.map((entry) => entry.name))).toEqual(["Alpha plan"]);
});

test("deleting a saved build does not carry its name onto the build that becomes selected", async ({ page }) => {
  await ready(page);
  await openSection(page, "Saved Builds");
  for (const name of ["First", "Second"]) {
    await page.getByLabel("Name", { exact: true }).fill(name);
    await page.getByRole("button", { name: "Save new", exact: true }).click();
  }
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  await expect(page.getByLabel("Name", { exact: true })).toHaveValue("First");
});

test("exporting a build says so, and the share button is named for what it copies", async ({ page }) => {
  await ready(page);
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await expect(page.getByRole("button", { name: "Copy share text", exact: true })).toBeEnabled();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export", exact: true }).click();
  await download;
  await expect(page.getByText(/^Exported .* as a JSON file to your Downloads folder\.$/)).toBeVisible();
});

test("a crashed workspace recovers on navigation and a crashed Build Detail on another selection or retry", async ({ page }) => {
  await ready(page);
  await search(page);
  await page.locator(".result-row").first().click();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    Object.assign(window, { goodControls: useDesktopStore.getState().compareControls });
    useDesktopStore.setState({ compareControls: null as never });
    useDesktopStore.getState().setWorkspace("compare");
  });
  await expect(page.getByRole("heading", { name: "This workspace could not be displayed" })).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState({ compareControls: (window as unknown as { goodControls: never }).goodControls });
    useDesktopStore.getState().setWorkspace("paths");
  });
  await expect(page.getByRole("heading", { name: "This workspace could not be displayed" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Paths" })).toBeVisible();

  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().setWorkspace("rankings");
    const [first] = useDesktopStore.getState().rows;
    Object.assign(window, { goodRow: first });
    useDesktopStore.setState({ selected: { ...first, aowRoute: { actions: 5 } as never } });
  });
  await expect(page.getByRole("heading", { name: "Build Detail could not be displayed" })).toBeVisible();
  // The same build with its data repaired: Try again, not a new selection, brings it back.
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState({ selected: (window as unknown as { goodRow: never }).goodRow });
  });
  await page.getByRole("button", { name: "Try again", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Build Detail could not be displayed" })).toHaveCount(0);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState({ selected: { ...(window as unknown as { goodRow: object }).goodRow, aowRoute: { actions: 5 } } as never });
  });
  await expect(page.getByRole("heading", { name: "Build Detail could not be displayed" })).toBeVisible();
  await page.locator(".result-row").nth(1).click();
  await expect(page.getByRole("heading", { name: "Build Detail could not be displayed" })).toHaveCount(0);
});

test("script errors reach the error strip, except the harmless resize-observer notice", async ({ page }) => {
  await ready(page);
  await page.evaluate(() => {
    window.dispatchEvent(new ErrorEvent("error", { message: "ResizeObserver loop completed with undelivered notifications." }));
  });
  await expect(page.locator(".error-strip")).toHaveCount(0);
  await page.evaluate(() => {
    window.dispatchEvent(new ErrorEvent("error", { message: "Uncaught Error: native bridge lost", error: new Error("native bridge lost") }));
  });
  await expect(page.locator('.error-strip[role="alert"]')).toContainText("native bridge lost");
});
