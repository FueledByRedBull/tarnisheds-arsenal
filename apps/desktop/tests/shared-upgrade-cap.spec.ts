import { expect, test } from "@playwright/test";
import { closeEditors, openEditor } from "./editors";

test("the shared upgrade cap note reads the cap from the profile rules", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("radiogroup", { name: "Game profile" }).getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental model", { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const catalog = useDesktopStore.getState().catalog!;
    const rules = { ...catalog.dataManifest.rules, standardMaxUpgrade: 12, somberMaxUpgrade: 12 };
    useDesktopStore.setState({ catalog: { ...catalog, dataManifest: { ...catalog.dataManifest, rules } } });
  });
  const editor = await openEditor(page, "Upgrades");
  await expect(editor.getByText(/one \+0 to \+12 reinforcement path/)).toBeVisible();
  await closeEditors(page);
});

test("the starter example states its one upgrade in both caps on a single-path profile", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("radiogroup", { name: "Game profile" }).getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental model", { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const start = api.startSearch;
    api.startSearch = async request => {
      Object.assign(window, { starterRequest: request });
      return start(request);
    };
  });
  await page.getByRole("button", { name: "Try an Uchigatana +3 example", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean((window as any).starterRequest))).toBe(true);
  const request = await page.evaluate(() => (window as any).starterRequest);
  expect(request).toMatchObject({ profileId: "convergence", standardMaxUpgrade: 3, somberMaxUpgrade: 3, exactUpgrade: true });
});
