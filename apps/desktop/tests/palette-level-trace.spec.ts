import { expect, test } from "@playwright/test";

test("the palette's level command opens Paths and traces the selected build", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "Command" }).fill("level 150");
  const plan = page.getByRole("option").filter({ hasText: /^.*Plan levels/ }).first();
  await expect(plan).toBeVisible();
  await page.keyboard.press("Enter");

  await expect(page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true })).toHaveAttribute("aria-current", "page");
  // The trace starts without pressing Trace paths.
  await expect(page.getByRole("table", { name: "Path steps", exact: true }).locator('[role="row"]').nth(1)).toBeVisible();
});
