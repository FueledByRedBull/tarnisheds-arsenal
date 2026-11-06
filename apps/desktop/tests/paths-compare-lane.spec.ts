import { expect, test } from "@playwright/test";
import { openSection } from "./editors";

test("Paths shows a loaded build's comparison target without Compare ever opening", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
  const savedTarget = await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const target = useDesktopStore.getState().rows[2];
    useDesktopStore.setState({ compareTarget: target });
    return { weaponName: target.weaponName, affinity: target.affinity };
  });
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.reload();
  await page.getByText("Snapshot loaded", { exact: true }).waitFor();
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
  await expect(page.locator(".path-lane").nth(1)).toContainText(`${savedTarget.weaponName} / ${savedTarget.affinity}`);
  await page.getByRole("button", { name: "Trace paths", exact: true }).click();
  await expect(page.locator(".path-lane").nth(1)).toContainText("steps, final");
  await expect(page.getByRole("columnheader", { name: /^Compare/ })).toContainText(`${savedTarget.weaponName} / ${savedTarget.affinity}`);
});

test("Paths tells the player where to choose a comparison", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
  await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
  await expect(page.locator(".path-lane").nth(1)).toContainText("Choose a comparison in Compare.");
});
