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
  await expect(page.getByText("Experimental model", { exact: true })).toBeVisible();
  await twoHanding(page).setChecked(false);
  const str = page.getByRole("spinbutton", { name: "STR", exact: true });
  await str.fill("43");
  await str.press("Enter");
  await expect.poll(() => storedQuery(page, "convergence")).toMatchObject({ profileId: "convergence", twoHanding: false, strStat: 43 });
  expect(await storedQuery(page, "vanilla")).toMatchObject({ profileId: "vanilla", twoHanding: true });

  // After a reload only storage remembers Vanilla's query; Convergence opens as it was left.
  await page.reload();
  await expect(page.getByText("Experimental model", { exact: true })).toBeVisible();
  await expect(page.getByText("Restored your The Convergence query. Search to rank it again.")).toBeVisible();
  await expect(page.getByText(/could not be read and was discarded/)).toHaveCount(0);
  await expect(str).toHaveValue("43");
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

test("a restored weapon's profile loads when the catalog arrives before it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().patchRequest({ weaponName: "Uchigatana" });
  });
  await expect.poll(() => page.evaluate(() => localStorage.getItem("tarnisheds-arsenal.query.v1.vanilla")))
    .toContain('"weaponName":"Uchigatana"');

  // On the desktop the catalog can arrive while the restored weapon's profile is still loading.
  await page.addInitScript(() => {
    let profileRequested!: () => void;
    const requested = new Promise<void>((resolve) => { profileRequested = resolve; });
    let catalogReturned!: () => void;
    const returned = new Promise<void>((resolve) => { catalogReturned = resolve; });
    let held = false;
    Object.assign(window, {
      isTauri: true,
      __TAURI_INTERNALS__: {
        async invoke(command: string, args?: Record<string, unknown>) {
          const { mockInvoke } = await import("/src/lib/browser-backend.ts");
          if (command === "get_catalog") {
            await requested;
            const catalog = await mockInvoke(command, args);
            catalogReturned();
            return catalog;
          }
          if (command === "get_weapon_profile" && !held) {
            held = true;
            profileRequested();
            await returned;
            await new Promise((resolve) => setTimeout(resolve, 100));
          }
          return mockInvoke(command, args);
        },
      },
    });
  });
  await page.reload();
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await expect(page.getByText(/^Requirements (clear|unmet)$/)).toBeVisible();
  await expect(page.getByText(/Weapon profile unavailable/)).toHaveCount(0);
});
