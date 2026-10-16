import { expect, test, type Page } from "@playwright/test";

const twoHanding = (page: Page) => page.getByRole("checkbox", { name: "Two-handing", exact: true });

async function storedQuery(page: Page, profileId: string) {
  return page.evaluate((key) => {
    const raw = localStorage.getItem(key);
    return raw === null ? null : JSON.parse(raw).request as { profileId: string; twoHanding: boolean };
  }, `tarnisheds-arsenal.query.v1.${profileId}`);
}

test("each profile's query survives a reload without touching the other profile", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await twoHanding(page).check();
  await page.reload();
  await expect(page.getByText("Restored your Vanilla query. Search to rank it again.")).toBeVisible();
  await expect(twoHanding(page)).toBeChecked();

  await page.getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
  await twoHanding(page).setChecked(false);
  await expect.poll(() => storedQuery(page, "convergence")).toMatchObject({ profileId: "convergence", twoHanding: false });
  expect(await storedQuery(page, "vanilla")).toMatchObject({ profileId: "vanilla", twoHanding: true });

  // After a reload only storage remembers Vanilla's query; Convergence opens as it was left.
  await page.reload();
  await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
  await expect(twoHanding(page)).not.toBeChecked();
  await page.getByRole("radio", { name: /Vanilla/ }).click();
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await expect(twoHanding(page)).toBeChecked();
});

test("an unreadable saved query is discarded with a notice", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await page.evaluate(() => localStorage.setItem("tarnisheds-arsenal.query.v1.vanilla", JSON.stringify({
    version: 1, request: { profileId: "convergence" }, lockedStatMode: false,
  })));
  await page.reload();
  await expect(page.getByText("Your saved Vanilla query could not be read and was discarded.")).toBeVisible();
  await expect(twoHanding(page)).not.toBeChecked();
});
