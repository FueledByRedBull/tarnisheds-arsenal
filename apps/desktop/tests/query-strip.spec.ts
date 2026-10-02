import { expect, test, type Page } from "@playwright/test";
import { openEditor, type EditorName } from "./editors";

async function ready(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
}

async function palette(page: Page, query: string) {
  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Edit anything", exact: true });
  await expect(dialog).toBeVisible();
  const input = dialog.getByRole("combobox", { name: "Command" });
  await expect(input).toBeFocused();
  await input.fill(query);
  return { dialog, input };
}

const state = (page: Page) => page.evaluate(async () => {
  const { useDesktopStore } = await import("/src/lib/state.ts");
  const { request, lockedStatMode, activeWorkspace } = useDesktopStore.getState();
  return { request, lockedStatMode, workspace: activeWorkspace };
});

test("strip tokens summarise the query and their editors apply edits in place", async ({ page }) => {
  await ready(page);
  const token = (name: string) => page.getByRole("button", { name: new RegExp(`^${name}: `) });
  await expect(token("Class")).toHaveAccessibleName("Class: Samurai");
  await expect(token("Objective")).toHaveAccessibleName("Objective: Max AR");
  await expect(token("Loadout")).toHaveAccessibleName("Loadout: Any weapon");
  await expect(token("Results")).toHaveAccessibleName("Results: Top 25");

  const classPanel = await openEditor(page, "Class");
  await classPanel.getByRole("combobox", { name: "Class" }).fill("Vagabond");
  await page.keyboard.press("Enter");
  await expect(token("Class")).toHaveAccessibleName("Class: Vagabond");
  await expect(page.getByRole("spinbutton", { name: "VIG", exact: true })).toHaveValue("15");
  await page.keyboard.press("Escape");
  await expect(classPanel).toBeHidden();
  await expect(token("Class")).toBeFocused();

  const objective = await openEditor(page, "Objective");
  await objective.getByRole("button", { name: "Bleed, then AR", exact: true }).click();
  await expect(objective).toBeHidden();
  await expect(token("Objective")).toHaveAccessibleName("Objective: Bleed, then AR");

  const loadout = await openEditor(page, "Loadout");
  await loadout.getByRole("combobox", { name: "Weapon", exact: true }).fill("Uchigatana");
  await page.keyboard.press("Enter");
  await expect(token("Loadout")).toHaveAccessibleName(/^Loadout: Uchigatana/);

  const results = await openEditor(page, "Results");
  await expect(loadout).toBeHidden();
  await results.getByRole("spinbutton", { name: "Top Results" }).fill("10");
  await results.getByRole("spinbutton", { name: "Top Results" }).press("Enter");
  await expect(token("Results")).toHaveAccessibleName(/^Results: Top 10/);

  await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
  expect((await state(page)).request).toMatchObject({ className: "Vagabond", objective: "max_ar_plus_bleed",
    weaponName: "Uchigatana", topK: 10, twoHanding: true });
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText(/\d+ ranked rows?/)).toBeVisible();
});

test("ribbon stats edit directly with the class base as the floor", async ({ page }) => {
  await ready(page);
  const str = page.getByRole("spinbutton", { name: "STR", exact: true });
  await str.fill("40");
  await str.press("Enter");
  await expect(str).toHaveValue("40");
  await expect(page.getByRole("textbox", { name: "Level", exact: true })).toHaveValue("37");
  await str.fill("3");
  await str.press("Enter");
  await expect(str).toHaveValue("12");
  await str.press("ArrowUp");
  await expect(str).toHaveValue("13");
});

test("Ctrl+K applies typed edits, actions and navigation", async ({ page }) => {
  await ready(page);
  let { dialog, input } = await palette(page, "str 40");
  await expect(dialog.getByRole("option").first()).toHaveText(/Set STR to 40/);
  await expect(dialog.getByRole("option").first()).toHaveAttribute("aria-selected", "true");
  await input.press("Enter");
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("spinbutton", { name: "STR", exact: true })).toHaveValue("40");

  ({ dialog, input } = await palette(page, "lock dex 30"));
  await input.press("Enter");
  expect((await state(page))).toMatchObject({ lockedStatMode: true, request: { lockDex: 30 } });
  await expect(page.locator(".active-lock-warning")).toContainText("DEX 30");

  ({ dialog, input } = await palette(page, "clear locks"));
  await input.press("Enter");
  expect((await state(page))).toMatchObject({ lockedStatMode: false, request: { lockDex: null } });

  ({ dialog, input } = await palette(page, "uchi"));
  await expect(dialog.getByRole("option").first()).toHaveText(/Weapon: Uchigatana/);
  await input.press("Enter");
  await expect(page.getByRole("button", { name: /^Loadout: Uchigatana/ })).toBeVisible();

  ({ dialog, input } = await palette(page, "bleed"));
  await dialog.getByRole("option", { name: /Objective: Bleed, then AR/ }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("button", { name: "Objective: Bleed, then AR" })).toBeVisible();

  ({ dialog, input } = await palette(page, "top 5"));
  await input.press("Enter");
  await expect(page.getByRole("button", { name: /^Results: Top 5/ })).toBeVisible();

  ({ dialog, input } = await palette(page, "search"));
  await input.press("Enter");
  await expect(page.getByText(/\d+ ranked rows?/)).toBeVisible();

  ({ dialog, input } = await palette(page, "compare"));
  await expect(dialog.getByRole("option", { name: /Go to Compare/ })).toBeVisible();
  await input.press("ArrowDown");
  await input.press("ArrowUp");
  await input.press("Enter");
  expect((await state(page)).workspace).toBe("compare");
});

test("the palette explains empty results and closes without side effects", async ({ page }) => {
  await ready(page);
  const before = await state(page);
  const { dialog, input } = await palette(page, "zzzz qqqq");
  await expect(dialog.getByRole("option")).toHaveCount(0);
  await expect(dialog.getByRole("status")).toHaveText(/No command matches/);
  await input.press("Enter");
  await expect(dialog).toBeVisible();
  await input.press("Escape");
  await expect(dialog).toBeHidden();
  expect(await state(page)).toEqual(before);

  await page.getByRole("button", { name: "Edit anything", exact: true }).click();
  await expect(dialog).toBeVisible();
  // A dismissed query comes back selected, so typing replaces it.
  await expect(input).toHaveValue("zzzz qqqq");
  expect(await input.evaluate((node: HTMLInputElement) => node.selectionStart === 0 && node.selectionEnd === node.value.length)).toBe(true);
  await page.mouse.click(4, 4);
  await expect(dialog).toBeHidden();
  await page.keyboard.press("Control+k");
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Control+k");
  await expect(dialog).toBeHidden();
});

for (const [width, height] of [[1200, 720], [1650, 950]]) {
  test(`every strip editor opens inside the window at ${width}x${height}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await ready(page);
    for (const editor of ["Class", "Objective", "Loadout", "Upgrades", "Scaling", "Results", "Limits"] satisfies EditorName[]) {
      const panel = await openEditor(page, editor);
      await expect.poll(() => panel.evaluate((node) => {
        const box = node.getBoundingClientRect();
        return box.width > 200 && box.left >= 0 && box.top >= 0 && box.right <= window.innerWidth && box.bottom <= window.innerHeight
          && node.scrollWidth <= node.clientWidth + 1;
      })).toBe(true);
      await page.keyboard.press("Escape");
      await expect(panel).toBeHidden();
    }
  });
}

test("Convergence summarises custom stats and hides class-budget commands", async ({ page }) => {
  await ready(page);
  await page.getByRole("radiogroup", { name: "Game profile" }).getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByRole("button", { name: "Class: Custom stats" })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Upgrades: / })).toHaveAccessibleName(/\+15/);
  await expect(page.getByRole("button", { name: /^Scaling: / })).toHaveAccessibleName(/No Scadutree/);
  const { dialog, input } = await palette(page, "str 40");
  await expect(dialog.getByRole("option")).toHaveCount(1);
  await input.fill("optimize class");
  await expect(dialog.getByRole("option", { name: /Optimize class/ })).toHaveCount(0);
  await input.fill("blessing 5");
  await expect(dialog.getByRole("option", { name: /Scadutree Blessing/ })).toHaveCount(0);
});
