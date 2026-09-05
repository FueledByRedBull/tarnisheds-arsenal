import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { createServer } from "node:net";
import { randomUUID } from "node:crypto";
import { join } from "node:path";

const executable = process.argv[2];
if (!executable || !existsSync(executable)) {
  throw new Error("Usage: node scripts/smoke-packaged.mjs <packaged-executable>");
}

const startupAttempts = positiveIntegerFromEnv("PACKAGED_SMOKE_START_ATTEMPTS", 2);
const startupTimeoutMs = positiveIntegerFromEnv("PACKAGED_SMOKE_STARTUP_TIMEOUT_MS", 90_000);
const retryCooldownMs = positiveIntegerFromEnv("PACKAGED_SMOKE_RETRY_COOLDOWN_MS", 5_000);
const vanillaCompareBenchKey = "tarnisheds-arsenal.compareBench.v1.vanilla";

let session;
let previousCompareBench;
try {
  session = await launchPackagedApp(
    executable,
    startupAttempts,
    startupTimeoutMs,
    retryCooldownMs,
  );
  const { page } = session;
  page.setDefaultTimeout(30_000);

  await page.getByText("Full model ready", { exact: true }).waitFor();
  previousCompareBench = await page.evaluate((key) => localStorage.getItem(key), vanillaCompareBenchKey);
  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("96");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  const highLevelFirst = page.locator(".result-row-full").first();
  await highLevelFirst.waitFor();
  const pin = highLevelFirst.getByRole("button", { name: "Compare", exact: true });
  if (await pin.getAttribute("aria-pressed") !== "true") await pin.click();

  const profileSwitch = page.getByRole("radiogroup", { name: "Game profile" });
  await profileSwitch.getByRole("radio", { name: /Convergence/ }).click();
  await page.getByText("Experimental fixed-stat model", { exact: true }).waitFor();
  if (await page.getByRole("combobox", { name: "Class", exact: true }).inputValue() !== "Custom stats") {
    throw new Error("Convergence substituted a starting-class budget for fixed stats");
  }
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await page.locator(".result-row-full").first().waitFor();
  await profileSwitch.getByRole("radio", { name: /Vanilla/ }).click();
  await page.getByText("Full model ready", { exact: true }).waitFor();
  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("12");

  await page.getByRole("button", { name: "Search", exact: true }).click();
  const fourth = page.locator(".result-row-full").nth(3);
  await fourth.waitFor();
  const selectedWeapon = (await fourth.locator(".weapon-cell strong").textContent())?.trim();
  if (!selectedWeapon) throw new Error("rank-four selection did not expose a weapon name");
  await fourth.click();
  await page.locator(".selected-build strong").getByText(selectedWeapon, { exact: true }).waitFor();

  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await page.getByText("Comparison current", { exact: true }).waitFor();
  await page.getByRole("button", { name: "Compare Type", exact: true }).click();
  await page.getByRole("group", { name: "Compare Type", exact: true })
    .getByRole("checkbox", { name: /^Axe\b/ })
    .check();
  await page.keyboard.press("Escape");
  const bestTypeLane = page.locator(".compare-lane", { hasText: "Best Axe" });
  await bestTypeLane.locator("strong").waitFor();
  const bestTypeWeapon = (await bestTypeLane.locator("strong").textContent())?.trim();
  if (!bestTypeWeapon) throw new Error("best-Axe comparison did not resolve a target");
  await page.getByText("Comparison current", { exact: true }).waitFor();

  await page.getByRole("navigation").getByRole("button", { name: "Paths" }).click();
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("10");
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await page.getByRole("grid", { name: "Path steps" }).locator('[role="row"]').nth(1).waitFor();

  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch" }).click();
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("10");
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await page.getByRole("grid", { name: "Affinity watch rankings" }).locator('[role="row"]').nth(1).waitFor();

  const presetName = `Release verification ${Date.now()}`;
  await page.getByRole("textbox", { name: "Name", exact: true }).fill(presetName);
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByText(`Saved ${presetName}.`, { exact: true }).waitFor();
  await page.reload();
  await page.getByRole("combobox", { name: "Saved", exact: true }).selectOption({ label: `${presetName} — vanilla · current data` });
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await page.getByText(`Loaded ${presetName}.`, { exact: true }).waitFor();
  await page.locator(".selected-build strong").getByText(selectedWeapon, { exact: true }).waitFor();
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  await page.getByText(`Deleted ${presetName}.`, { exact: true }).waitFor();
  await page.reload();
  await page.getByText("Full model ready", { exact: true }).waitFor();
  const savedBuilds = page.getByRole("combobox", { name: "Saved", exact: true });
  await savedBuilds.locator('option[value=""]').waitFor({ state: "attached" });
  if (await savedBuilds.inputValue() !== "") {
    throw new Error("packaged smoke preset remained selected after deletion and reload");
  }
  if (await savedBuilds.locator(`option:has-text("${presetName}")`).count()) {
    throw new Error("packaged smoke preset survived explicit cleanup");
  }

  process.stdout.write(`PACKAGED_SMOKE_PASSED ${JSON.stringify({ selectedWeapon, bestTypeWeapon, presetName })}\n`);
} catch (error) {
  const output = session?.output().trim();
  const pageState = session?.page
    ? await session.page.locator(".analysis-state, .error-banner").allTextContents().catch(() => [])
    : [];
  const suffix = [
    pageState.length ? `\nPackaged page state:\n${pageState.join("\n")}` : "",
    output ? `\nPackaged app output:\n${output.slice(-4000)}` : "",
  ].join("");
  throw new Error(`${error instanceof Error ? error.message : String(error)}${suffix}`);
} finally {
  if (session?.page && previousCompareBench !== undefined) {
    await session.page.evaluate(({ key, value }) => {
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    }, { key: vanillaCompareBenchKey, value: previousCompareBench }).catch(() => {});
  }
  await stopSession(session);
}

async function launchPackagedApp(executablePath, attempts, timeoutMs, cooldownMs) {
  const failures = [];
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    const localAppData = process.env.LOCALAPPDATA;
    if (!localAppData) throw new Error("LOCALAPPDATA is required for packaged smoke isolation");
    const profileToken = `tarnisheds-arsenal-smoke-${randomUUID()}`;
    const profileDirectory = join(localAppData, "main", profileToken);
    const port = await reserveLoopbackPort();
    const endpoint = `http://127.0.0.1:${port}`;
    let output = "";
    let exit = null;
    let browser;
    const child = spawn(executablePath, [
      `--packaged-smoke-port=${port}`,
      `--packaged-smoke-profile=${profileToken}`,
    ], {
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    process.stdout.write(
      `PACKAGED_SMOKE_START attempt=${attempt}/${attempts} pid=${child.pid ?? "unknown"} port=${port} timeoutMs=${timeoutMs}\n`,
    );
    child.stdout.on("data", (chunk) => { output += chunk.toString(); });
    child.stderr.on("data", (chunk) => { output += chunk.toString(); });
    child.once("exit", (code, signal) => { exit = { code, signal }; });
    const candidate = {
      browser,
      child,
      exit: () => exit,
      output: () => output,
      page: undefined,
      profileDirectory,
    };

    try {
      await waitForEndpoint(`${endpoint}/json/version`, timeoutMs, () => exit);
      browser = await chromium.connectOverCDP(endpoint);
      candidate.browser = browser;
      candidate.page = await waitForAppPage(browser, 30_000, () => exit);
      return candidate;
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      const childState = exit
        ? `exited with code ${String(exit.code)} and signal ${String(exit.signal)}`
        : `still running as PID ${child.pid ?? "unknown"}`;
      const captured = output.trim();
      failures.push(
        `attempt ${attempt}/${attempts} on ${endpoint}: ${reason}; process ${childState}`
        + (captured ? `\n${captured.slice(-2000)}` : ""),
      );
      await stopSession(candidate);
      if (attempt < attempts) {
        process.stdout.write(`PACKAGED_SMOKE_RETRY cooldownMs=${cooldownMs}\n`);
        await new Promise((resolve) => setTimeout(resolve, cooldownMs));
      }
    }
  }
  throw new Error(`packaged WebView2 startup failed after ${attempts} attempts:\n${failures.join("\n")}`);
}

async function stopSession(sessionToStop) {
  if (!sessionToStop) return;
  await sessionToStop.browser?.close().catch(() => undefined);
  if (!sessionToStop.exit() && !sessionToStop.child.killed) sessionToStop.child.kill();
  await Promise.race([
    new Promise((resolve) => {
      if (sessionToStop.exit()) resolve();
      else sessionToStop.child.once("exit", resolve);
    }),
    new Promise((resolve) => setTimeout(resolve, 5_000)),
  ]);
  if (!sessionToStop.exit()) sessionToStop.child.kill("SIGKILL");
  await rm(sessionToStop.profileDirectory, {
    recursive: true,
    force: true,
    maxRetries: 3,
    retryDelay: 250,
  }).catch(() => undefined);
}

async function reserveLoopbackPort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    server.close();
    throw new Error("could not reserve a loopback port for packaged smoke testing");
  }
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  return address.port;
}

async function waitForEndpoint(url, timeoutMs, getExit) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const exit = getExit();
    if (exit) {
      throw new Error(`packaged app exited before WebView2 was ready (code ${String(exit.code)}, signal ${String(exit.signal)})`);
    }
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(2_000) });
      if (response.ok) {
        const metadata = await response.json();
        if (typeof metadata.webSocketDebuggerUrl === "string") return;
      }
    } catch {
      // WebView2 has not opened its local debugging endpoint yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timed out after ${timeoutMs} ms waiting for packaged WebView2 endpoint ${url}`);
}

async function waitForAppPage(connectedBrowser, timeoutMs, getExit) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const exit = getExit();
    if (exit) {
      throw new Error(`packaged app exited before its page was ready (code ${String(exit.code)}, signal ${String(exit.signal)})`);
    }
    for (const context of connectedBrowser.contexts()) {
      for (const page of context.pages()) {
        if (await page.locator(".desktop-shell").count()) return page;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("packaged WebView2 page did not expose the application shell");
}

function positiveIntegerFromEnv(name, fallback) {
  const raw = process.env[name];
  if (!raw) return fallback;
  const parsed = Number(raw);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}
