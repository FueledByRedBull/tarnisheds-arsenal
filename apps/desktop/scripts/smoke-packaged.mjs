import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";

const executable = process.argv[2];
if (!executable || !existsSync(executable)) {
  throw new Error("Usage: node scripts/smoke-packaged.mjs <packaged-executable>");
}

const port = 9400 + Math.floor(Math.random() * 400);
const endpoint = `http://127.0.0.1:${port}`;
const child = spawn(executable, [], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  },
  stdio: ["ignore", "pipe", "pipe"],
  windowsHide: true,
});
let output = "";
child.stdout.on("data", (chunk) => { output += chunk.toString(); });
child.stderr.on("data", (chunk) => { output += chunk.toString(); });

let browser;
try {
  await waitForEndpoint(`${endpoint}/json/version`, 60_000);
  browser = await chromium.connectOverCDP(endpoint);
  const page = await waitForAppPage(browser, 30_000);
  page.setDefaultTimeout(30_000);

  await page.getByRole("button", { name: "Search", exact: true }).click();
  await page.getByText(/\d+ ranked rows/).waitFor();
  const fourth = page.locator(".result-row-full").nth(3);
  await fourth.click();
  const selectedWeapon = (await fourth.locator(".weapon-cell strong").textContent())?.trim();
  if (!selectedWeapon) throw new Error("rank-four selection did not expose a weapon name");
  await page.locator(".selected-build strong").getByText(selectedWeapon, { exact: true }).waitFor();

  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await page.getByText("Comparison current", { exact: true }).waitFor();

  await page.getByRole("navigation").getByRole("button", { name: "Paths" }).click();
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("10");
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await page.getByRole("grid", { name: "Path steps" }).locator('[role="row"]').nth(1).waitFor();

  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch" }).click();
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("10");
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await page.getByRole("grid", { name: "Affinity watch rankings" }).locator('[role="row"]').nth(1).waitFor();

  const presetName = `Packaged smoke ${Date.now()}`;
  await page.getByRole("textbox", { name: "Name", exact: true }).fill(presetName);
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByText(`Saved ${presetName}.`, { exact: true }).waitFor();
  await page.reload();
  await page.getByRole("combobox", { name: "Saved", exact: true }).selectOption({ label: `${presetName} — current data` });
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await page.getByText(`Loaded ${presetName}.`, { exact: true }).waitFor();
  await page.locator(".selected-build strong").getByText(selectedWeapon, { exact: true }).waitFor();

  process.stdout.write(`PACKAGED_SMOKE_PASSED ${JSON.stringify({ selectedWeapon, presetName })}\n`);
} catch (error) {
  const suffix = output.trim() ? `\nPackaged app output:\n${output.slice(-4000)}` : "";
  throw new Error(`${error instanceof Error ? error.message : String(error)}${suffix}`);
} finally {
  await browser?.close().catch(() => undefined);
  if (!child.killed) child.kill();
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    new Promise((resolve) => setTimeout(resolve, 5_000)),
  ]);
  if (!child.killed) child.kill("SIGKILL");
}

async function waitForEndpoint(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // WebView2 has not opened its local debugging endpoint yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timed out waiting for packaged WebView2 endpoint ${url}`);
}

async function waitForAppPage(connectedBrowser, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const context of connectedBrowser.contexts()) {
      for (const page of context.pages()) {
        if (await page.locator(".desktop-shell").count()) return page;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("packaged WebView2 page did not expose the application shell");
}
