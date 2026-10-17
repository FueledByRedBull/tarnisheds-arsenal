import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

// The browser preview stands in for the native backend in every other spec, so its identity
// and job rules are checked against the shipped snapshots and the native JobRegistry.
const repoFile = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");

test("the browser preview reports the shipped snapshot identities", async ({ page }) => {
  await page.goto("/");
  const profiles = await page.evaluate(async () => (await import("/src/lib/api.ts")).api.profiles());
  const exactModel = /format!\("\{snapshot_model\}\/(exact-v\d+)"\)/.exec(repoFile("core/er_optimizer_core/src/lib.rs"))?.[1];
  expect(exactModel).toBeTruthy();
  for (const path of ["data/phase1/manifest.json", "data/profiles/convergence/manifest.json"]) {
    const shipped = JSON.parse(repoFile(path));
    const mock = profiles.find((profile) => profile.profile.id === shipped.profile.id);
    expect(mock, shipped.profile.id).toBeDefined();
    const pick = (source: Record<string, unknown>, keys: object) =>
      Object.fromEntries(Object.keys(keys).map((key) => [key, source[key]]));
    expect(mock).toEqual({
      ...pick(shipped, mock!),
      modelVersion: `${shipped.modelVersion}/${exactModel}`,
      capabilities: pick(shipped.capabilities, mock!.capabilities),
      rules: pick(shipped.rules, mock!.rules),
    });
  }
});

test("the browser preview runs one job at a time and cancels cooperatively", async ({ page }) => {
  await page.goto("/");
  const observed = await page.evaluate(async () => {
    // Called directly: api.ts loads the mock lazily, and that import may yield a task.
    const { mockInvoke } = await import("/src/lib/browser-backend.ts");
    type Status = { finished: { cancelled: boolean; error: string | null; paths: unknown[] } | null } | null;
    const start = () => mockInvoke<{ jobId: string }>("start_path_preview", { request: { requests: [] } });
    const status = (jobId: string) => mockInvoke<Status>("get_path_preview_status", { jobId });
    const { jobId } = await start();
    const second = await start().then(() => "started", (error: Error) => error.message);
    const accepted = await mockInvoke<boolean>("cancel_path_preview", { jobId });
    const afterCancel = await status(jobId);
    let terminal = afterCancel;
    for (let polls = 0; !terminal?.finished && polls < 200; polls += 1) {
      await new Promise((resolve) => setTimeout(resolve, 5));
      terminal = await status(jobId);
    }
    const restart = await start();
    return { second, accepted, pendingAfterCancel: afterCancel?.finished ?? null, terminal: terminal?.finished, restarted: restart.jobId !== jobId };
  });
  expect(observed.second).toBe("path job is already running. Stop or wait for it before starting another.");
  expect(observed.accepted).toBe(true);
  expect(observed.pendingAfterCancel).toBeNull();
  expect(observed.terminal).toMatchObject({ cancelled: true, error: null, paths: [] });
  expect(observed.restarted).toBe(true);
});

test("the browser preview rejects commands the native app does not register", async ({ page }) => {
  await page.goto("/");
  const message = await page.evaluate(async () => {
    const { mockInvoke } = await import("/src/lib/browser-backend.ts");
    return mockInvoke("get_unknown_status").then(() => "resolved", (error: Error) => error.message);
  });
  expect(message).toBe("Unknown command get_unknown_status");
});
