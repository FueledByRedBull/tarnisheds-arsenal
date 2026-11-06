import { expect, test } from "@playwright/test";
import { openSection } from "./editors";

test("Migrate data names the saved weapon it cleared, not filters", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await expect(page.getByText(/Saved Blood Uchigatana \+25/)).toBeVisible();

  await page.evaluate(() => {
    const presetKey = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."));
    if (!presetKey) throw new Error("saved preset was not created");
    const preset = JSON.parse(localStorage.getItem(presetKey) ?? "null");
    preset.dataVersion = "1:old-dataset:old-model";
    preset.request.weaponName = "Vanished Blade";
    localStorage.setItem(presetKey, JSON.stringify(preset));
    const indexKey = "tarnisheds-arsenal.savedBuildIndex.v1";
    const index = JSON.parse(localStorage.getItem(indexKey) ?? "null");
    index.builds[0].dataVersion = preset.dataVersion;
    localStorage.setItem(indexKey, JSON.stringify(index));
  });
  await page.reload();

  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Migrate data", exact: true }).click();
  await expect(page.getByText(/Migrated .* and verified/)).toContainText("Cleared missing names: weapon 'Vanished Blade' no longer exists.");
  await expect(page.getByText(/Cleared filters/)).toHaveCount(0);
});
