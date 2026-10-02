import { expect, test, type Page } from "@playwright/test";
import { openEditor } from "./editors";

async function searched(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
}

const request = (page: Page) => page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().request);

test("Ctrl+Z and Ctrl+Y step through query edits with named buttons", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  const undo = page.getByRole("group", { name: "Query history" }).getByRole("button", { name: /^Undo/ });
  const redo = page.getByRole("group", { name: "Query history" }).getByRole("button", { name: /^Redo/ });
  await expect(undo).toBeDisabled();

  const str = page.getByRole("spinbutton", { name: "STR", exact: true });
  await str.fill("40");
  await str.press("Enter");
  await (await openEditor(page, "Objective")).getByRole("button", { name: "Bleed, then AR", exact: true }).click();
  await expect(undo).toHaveAccessibleName("Undo Objective Max AR → Bleed, then AR");

  await page.locator("body").click({ position: { x: 4, y: 300 } });
  await page.keyboard.press("Control+z");
  await expect(page.getByRole("button", { name: "Objective: Max AR" })).toBeVisible();
  await expect(page.getByRole("group", { name: "Query history" }).getByRole("status")).toHaveText("Undid Objective Max AR → Bleed, then AR");
  await expect(undo).toHaveAccessibleName("Undo STR 12 → 40");
  await undo.click();
  await expect(str).toHaveValue("12");
  await expect(undo).toBeDisabled();

  await page.keyboard.press("Control+y");
  await expect(str).toHaveValue("40");
  await page.keyboard.press("Control+Shift+z");
  await expect(page.getByRole("button", { name: "Objective: Bleed, then AR" })).toBeVisible();
  await expect(redo).toBeDisabled();

  // The palette offers the same steps.
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "Command" }).fill("undo");
  await expect(page.getByRole("option").first()).toContainText("Objective Max AR → Bleed, then AR");
  await page.keyboard.press("Enter");
  expect((await request(page)).objective).toBe("max_ar");

  // A profile switch starts a fresh history.
  await page.getByRole("radiogroup", { name: "Game profile" }).getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
  await expect(undo).toBeDisabled();
  await expect(redo).toBeDisabled();
});

test("text fields keep their own undo", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  await (await openEditor(page, "Objective")).getByRole("button", { name: "Bleed, then AR", exact: true }).click();
  const str = page.getByRole("spinbutton", { name: "STR", exact: true });
  await str.focus();
  await page.keyboard.press("Control+z");
  expect((await request(page)).objective).toBe("max_ar_plus_bleed");
});

test("rankings show movement and value changes since the previous search", async ({ page }) => {
  await searched(page);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const [first, second, third, fourth] = useDesktopStore.getState().rows;
    useDesktopStore.getState().setRows([
      { ...third, ar: { ...third.ar, total: third.ar.total + 12.5 } },
      first,
      second,
      { ...fourth, aowName: "Different skill" },
    ]);
  });
  const rows = page.locator(".result-row-full");
  await expect(rows.nth(0).locator(".rank-move")).toHaveText("2");
  await expect(rows.nth(0).locator(".rank-move")).toHaveClass(/up/);
  await expect(rows.nth(0).locator(".metric-delta")).toHaveText("+12.5");
  await expect(rows.nth(1).locator(".rank-move")).toHaveClass(/down/);
  await expect(rows.nth(3).locator(".rank-move")).toHaveText("New");
  await expect(page.getByRole("row", { name: /rank 1, up 2 since the previous search$/ })).toBeVisible();

  // Running the same search again shows no movement.
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().setRows([...useDesktopStore.getState().rows]);
  });
  await expect(page.locator(".rank-move, .metric-delta")).toHaveCount(0);
});

test("result columns sort without renumbering ranks", async ({ page }) => {
  await searched(page);
  // Rows keep rank order in the DOM and sort with CSS order, so read them in on-screen order.
  const ranks = () => page.locator(".result-row-full .rank-cell").evaluateAll((cells) => cells
    .sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top)
    .map((cell) => cell.textContent));
  const ar = page.getByRole("columnheader", { name: "AR", exact: true });
  await ar.getByRole("button").click();
  await expect(ar).toHaveAttribute("aria-sort", "descending");
  expect(await ranks()).toEqual(["1", "2", "3", "4"]);
  await ar.getByRole("button").click();
  await expect(ar).toHaveAttribute("aria-sort", "ascending");
  expect(await ranks()).toEqual(["4", "3", "2", "1"]);
  await expect(page.getByRole("button", { name: "Show highest first" })).toBeVisible();
  await page.getByRole("button", { name: "Show highest first" }).click();
  await expect(ar).toHaveAttribute("aria-sort", "descending");

  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const rows = useDesktopStore.getState().rows;
    useDesktopStore.getState().setRows([rows[0], rows[1], { ...rows[2], ar: { ...rows[2].ar, total: 999 } }, rows[3]]);
  });
  expect(await ranks()).toEqual(["3", "1", "2", "4"]);
  const rank = page.getByRole("columnheader", { name: "Rank", exact: true });
  await rank.getByRole("button").click();
  await expect(rank).toHaveAttribute("aria-sort", "ascending");
  await expect(ar).not.toHaveAttribute("aria-sort");
  expect(await ranks()).toEqual(["1", "2", "3", "4"]);
});

test("arrow keys move between ranked rows and Build Detail shows the rank", async ({ page }) => {
  await searched(page);
  const rows = page.locator(".result-row-full");
  await rows.first().focus();
  await page.keyboard.press("ArrowDown");
  await expect(rows.nth(1)).toBeFocused();
  await page.keyboard.press("End");
  await expect(rows.nth(3)).toBeFocused();
  await page.keyboard.press("Home");
  await expect(rows.nth(0)).toBeFocused();
  await expect(page.locator(".rank-context")).toHaveText("Rank 1 of 4 · best for this query");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(rows.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".rank-context")).toHaveText("Rank 2 of 4 · 30.0 behind #1");
});

test("shortcuts search, switch workspaces and list themselves", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  const nav = page.getByRole("navigation");
  await page.keyboard.press("Control+2");
  await expect(nav.getByRole("button", { name: "Rankings", exact: true })).toHaveAttribute("aria-current", "page");

  await page.keyboard.press("Control+Enter");
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.keyboard.press("Control+2");
  await expect(nav.getByRole("button", { name: "Compare", exact: true })).toHaveAttribute("aria-current", "page");
  await page.keyboard.press("Control+3");
  await expect(nav.getByRole("button", { name: "Paths", exact: true })).toHaveAttribute("aria-current", "page");
  await page.keyboard.press("Control+1");
  await expect(nav.getByRole("button", { name: "Rankings", exact: true })).toHaveAttribute("aria-current", "page");
  await expect(nav.getByRole("button", { name: "Compare", exact: true })).toHaveAttribute("title", "Compare (Ctrl+2)");

  await page.keyboard.press("?");
  const sheet = page.getByRole("dialog", { name: "Keyboard shortcuts" });
  await expect(sheet).toBeVisible();
  await expect(sheet.getByRole("region", { name: "Anywhere" })).toContainText("Undo the last query change");
  await page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();

  // Typing a "?" into a field stays typing.
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "Command" }).fill("shortcuts");
  await page.keyboard.press("Enter");
  await expect(sheet).toBeVisible();
  await sheet.getByRole("button", { name: "Close keyboard shortcuts" }).click();
  await expect(sheet).toBeHidden();
});

test("the palette highlights matches and recalls recent commands", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  await page.keyboard.press("Control+k");
  const input = page.getByRole("combobox", { name: "Command" });
  await input.fill("uchi");
  await expect(page.getByRole("option").first().locator("mark")).toHaveText("Uchi");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Control+k");
  await input.fill("");
  await expect(page.getByRole("option").first()).toContainText("Recent");
  await expect(page.getByRole("option").first()).toContainText("Weapon: Uchigatana");
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("tarnisheds-arsenal.recentCommands.v1")!))).toEqual(["weapon-Uchigatana"]);
});

test("closing the palette by clicking a field keeps focus on that field", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  const opener = page.getByRole("button", { name: "Edit anything", exact: true });
  const palette = page.getByRole("dialog", { name: "Edit anything", exact: true });
  await opener.click();
  await expect(palette).toBeVisible();
  const vig = page.getByRole("spinbutton", { name: "VIG", exact: true });
  await vig.click();
  await expect(palette).toBeHidden();
  await expect(vig).toBeFocused();

  // Escape has no new target, so focus goes back to where the palette was opened from.
  await opener.click();
  await expect(palette).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(palette).toBeHidden();
  await expect(opener).toBeFocused();
});

test("Ctrl+K leaves an open shortcut list in charge of the keyboard", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  await page.keyboard.press("?");
  const sheet = page.getByRole("dialog", { name: "Keyboard shortcuts" });
  await expect(sheet).toBeVisible();
  await page.keyboard.press("Control+k");
  await expect(page.locator(".command-palette:popover-open")).toHaveCount(0);
  await expect(sheet).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog", { name: "Edit anything", exact: true })).toBeVisible();
});

test("Build Detail never calls a rounded gap an exact tie", async ({ page }) => {
  await searched(page);
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const [first, second, ...rest] = useDesktopStore.getState().rows;
    useDesktopStore.getState().setRows([first, { ...second, ar: { ...second.ar, total: first.ar.total - 0.01 } }, ...rest]);
  });
  await page.locator(".result-row-full").nth(1).click();
  await expect(page.locator(".rank-context")).toHaveText("Rank 2 of 4 · less than 0.05 behind #1");
});
