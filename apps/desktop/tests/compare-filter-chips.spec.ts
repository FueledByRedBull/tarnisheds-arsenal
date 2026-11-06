import { expect, test } from "@playwright/test";
import { closeEditors, openCompare, openEditor } from "./editors";

test("removing the last Compare filter returns to ranked rivals, not an Automatic skill", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await page.locator(".result-row").first().click();
  await openCompare(page);

  const panel = await openEditor(page, "Comparison filters");
  await panel.locator(".checkbox-multi-trigger").and(page.getByRole("button", { name: /^Compare Type\b/ })).click();
  await panel.getByRole("group", { name: "Compare Type", exact: true }).getByRole("checkbox", { name: /^Katana\b/ }).click();
  await closeEditors(page);

  const chips = page.getByRole("list", { name: "Active comparison filters" });
  await expect(chips.getByRole("listitem")).toHaveText([/Type\s*Katana/, /Skill\s*Automatic/]);

  await chips.getByRole("button", { name: "Remove Type Katana", exact: true }).click();
  await expect(chips).toHaveCount(0);
  await expect(page.locator(".compare-default")).toHaveText("Current ranked rivals");
  await expect(page.getByRole("button", { name: "Use ranked rivals", exact: true })).toHaveCount(0);
});
