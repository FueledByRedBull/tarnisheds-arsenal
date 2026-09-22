import { expect } from "@playwright/test";
import { existsSync } from "node:fs";
import { launchPackagedApp, stopSession } from "./packaged-session.mjs";

const executable = process.argv[2];
if (!executable || !existsSync(executable)) {
  throw new Error("Usage: node scripts/smoke-packaged.mjs <packaged-executable>");
}

const startupAttempts = positiveIntegerFromEnv("PACKAGED_SMOKE_START_ATTEMPTS", 2);
const startupTimeoutMs = positiveIntegerFromEnv("PACKAGED_SMOKE_STARTUP_TIMEOUT_MS", 90_000);
const retryCooldownMs = positiveIntegerFromEnv("PACKAGED_SMOKE_RETRY_COOLDOWN_MS", 5_000);
const profileStorageKey = "tarnisheds-arsenal.gameProfile.v1";
const vanillaCompareBenchKey = "tarnisheds-arsenal.compareBench.v1.vanilla";

let session;
let previousCompareBench;
let smokeStage = "launch packaged app";
const smokeStartedAt = Date.now();

function markSmokeStage(stage) {
  smokeStage = stage;
  process.stdout.write(`PACKAGED_SMOKE_STAGE ${stage} elapsedMs=${Date.now() - smokeStartedAt}\n`);
}

try {
  session = await launchPackagedApp(
    executable,
    startupAttempts,
    startupTimeoutMs,
    retryCooldownMs,
  );
  const { page } = session;
  page.setDefaultTimeout(30_000);
  const policyErrors = [];
  let ipcResponses = 0;
  page.on("console", (message) => {
    const text = message.text();
    if (!text.includes("/__csp_probe__") && /content security policy|IPC custom protocol failed/i.test(text)) {
      policyErrors.push(text);
    }
  });
  page.on("response", (response) => {
    if (new URL(response.url()).hostname === "ipc.localhost") ipcResponses += 1;
  });
  markSmokeStage("verify production connection policy");
  await assertProductionConnections(page);
  const viewport = await page.evaluate(() => ({
    innerWidth: window.innerWidth,
    innerHeight: window.innerHeight,
    devicePixelRatio: window.devicePixelRatio,
  }));
  process.stdout.write(`PACKAGED_SMOKE_VIEWPORT ${JSON.stringify(viewport)}\n`);

  markSmokeStage("assert fresh packaged profile");
  const initialStoredProfile = await page.evaluate((key) => localStorage.getItem(key), profileStorageKey);
  if (initialStoredProfile !== null && initialStoredProfile !== "vanilla") {
    throw new Error(`packaged smoke profile was not fresh: stored profile ${JSON.stringify(initialStoredProfile)}`);
  }

  markSmokeStage("wait for vanilla model");
  await page.getByText("Snapshot loaded", { exact: true }).waitFor();
  if (await page.getByRole("radio", { name: /Vanilla/ }).getAttribute("aria-checked") !== "true") {
    throw new Error("packaged smoke did not start on the Vanilla profile");
  }
  previousCompareBench = await page.evaluate((key) => localStorage.getItem(key), vanillaCompareBenchKey);
  markSmokeStage("select exact-level high-level search");
  const exactLevelPolicy = page.getByRole("button", { name: "Use exact levels", exact: true });
  await exactLevelPolicy.click();
  if (await exactLevelPolicy.getAttribute("aria-pressed") !== "true") {
    throw new Error("packaged smoke could not select exact-level upgrade search");
  }
  markSmokeStage("run vanilla high-level search");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("96");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).press("Enter");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  const highLevelFirst = page.locator(".result-row-full").first();
  markSmokeStage("wait for vanilla high-level result");
  // Keep headroom for slower CI even though exact levels avoid the measured open-query cost.
  await highLevelFirst.waitFor({ timeout: 120_000 });
  await expect(highLevelFirst.getByRole("gridcell").nth(5)).toContainText("First");
  await expect(highLevelFirst.getByRole("gridcell").nth(5)).not.toContainText("Unavailable");
  const pin = highLevelFirst.getByRole("button", { name: /^Compare / });
  if (await pin.getAttribute("aria-pressed") !== "true") await pin.click();

  const profileSwitch = page.getByRole("radiogroup", { name: "Game profile" });
  markSmokeStage("switch to Convergence profile");
  await profileSwitch.getByRole("radio", { name: /Convergence/ }).click();
  markSmokeStage("wait for Convergence model");
  await page.getByText("Experimental fixed-stat model", { exact: true }).waitFor();
  if (await page.getByRole("combobox", { name: "Class", exact: true }).inputValue() !== "Custom stats") {
    throw new Error("Convergence substituted a starting-class budget for fixed stats");
  }
  await expect(page.getByRole("button", { name: "AoW First Hit", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Optimize class", exact: true })).toBeDisabled();
  for (const name of ["Compare", "Paths", "Affinity Watch"]) {
    await expect(page.getByRole("navigation").getByRole("button", { name, exact: true })).toBeDisabled();
  }
  markSmokeStage("save Convergence fixed stats");
  const convergencePresetName = `Convergence verification ${Date.now()}`;
  await page.getByRole("textbox", { name: "Name", exact: true }).fill(convergencePresetName);
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByText(`Saved ${convergencePresetName}.`, { exact: true }).waitFor();
  const convergenceTotal = await page.getByRole("textbox", { name: "Stat total", exact: true }).inputValue();
  markSmokeStage("run Convergence search");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  markSmokeStage("wait for Convergence result");
  const convergenceRow = page.locator(".result-row-full").first();
  await convergenceRow.waitFor();
  await expect(convergenceRow.getByRole("gridcell").nth(3)).toHaveText("+15");
  await expect(convergenceRow.getByRole("gridcell").nth(5)).toHaveText("Unavailable");
  const convergenceAr = await convergenceRow.locator(".ar-status-cell > strong").innerText();
  if (!(Number(convergenceAr.replace(/[^0-9.]/g, "")) > 0)) {
    throw new Error(`Convergence returned invalid weapon AR: ${convergenceAr}`);
  }
  await convergenceRow.click();
  await expect(page.locator(".metric-tile").filter({ hasText: "AoW model" })).toContainText("Unavailable");
  const convergenceWeapon = await convergenceRow.locator(".weapon-cell > strong").innerText();
  await expect(page.locator(".selected-build > strong")).toHaveText(convergenceWeapon);
  await page.getByRole("button", { name: "Update selected", exact: true }).click();
  await page.getByText(`Updated ${convergencePresetName}.`, { exact: true }).waitFor();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Stat total", exact: true })).toHaveValue(convergenceTotal);
  await expect(page.locator(".selected-build > strong")).toHaveText(convergenceWeapon);
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  process.stdout.write(`PACKAGED_SMOKE_CONVERGENCE ${JSON.stringify({ weapon: convergenceWeapon, ar: convergenceAr, upgrade: 15 })}\n`);
  markSmokeStage("switch back to Vanilla profile");
  await profileSwitch.getByRole("radio", { name: /Vanilla/ }).click();
  markSmokeStage("wait for Vanilla model after profile switch");
  await page.getByText("Snapshot loaded", { exact: true }).waitFor();
  await expect(page.getByRole("button", { name: "AoW First Hit", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Optimize class", exact: true })).toBeEnabled();
  await expect(page.getByRole("combobox", { name: "Class", exact: true })).toHaveValue("Samurai");
  markSmokeStage("select cap-exploration low-level search");
  const capExplorationPolicy = page.getByRole("button", { name: "Explore up to caps", exact: true });
  await capExplorationPolicy.click();
  if (await capExplorationPolicy.getAttribute("aria-pressed") !== "true") {
    throw new Error("packaged smoke could not select cap-exploration upgrade search");
  }
  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("12");
  await page.getByRole("spinbutton", { name: "STR", exact: true }).press("Enter");

  markSmokeStage("run vanilla rank-four search");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  const fourth = page.locator(".result-row-full").nth(3);
  markSmokeStage("wait for vanilla rank-four result");
  await fourth.waitFor();
  await expect(fourth.getByRole("gridcell").nth(5)).toContainText("First");
  const selectedWeapon = (await fourth.locator(".weapon-cell strong").textContent())?.trim();
  if (!selectedWeapon) throw new Error("rank-four selection did not expose a weapon name");
  await fourth.click();
  await page.locator(".selected-build strong").getByText(selectedWeapon, { exact: true }).waitFor();
  await expect(page.locator(".metric-tile").filter({ hasText: "Raw AoW" })).not.toContainText("Unavailable");

  markSmokeStage("open comparison");
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  markSmokeStage("wait for current comparison");
  await page.getByText("Comparison current", { exact: true }).waitFor();

  markSmokeStage("compute fixed-loadout AR / bleed tradeoffs");
  const tradeoffs = page.locator(".loadout-tradeoffs");
  await tradeoffs.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(tradeoffs.getByRole("status")).toContainText(/exact trade-off points? ready\./);
  await expect(tradeoffs.getByRole("table", { name: "Trade-off options", exact: true })).toBeVisible();
  await expect(tradeoffs.locator(".tradeoff-inspection")).toContainText(selectedWeapon);
  await tradeoffs.getByRole("spinbutton", { name: "Max AR sacrifice (%)" }).fill("3");
  await expect(tradeoffs.locator(".tradeoff-inspection")).toContainText("Full stat spread");
  const chosenTradeoffRow = tradeoffs.locator(".tradeoff-shortlist tbody tr.selected");
  await chosenTradeoffRow.waitFor();
  const chosenPointAr = (await chosenTradeoffRow.locator("td").nth(0).innerText()).trim();
  const chosenPointBleed = (await chosenTradeoffRow.locator("td").nth(1).innerText()).trim();
  await expect(page.locator(".selected-build strong")).toHaveText(selectedWeapon);
  const statText = await tradeoffs.locator(".tradeoff-inspection p").filter({ hasText: "Full stat spread" }).textContent();
  const exactStats = statText.match(/STR \d+ \/ DEX \d+ \/ INT \d+ \/ FAI \d+ \/ ARC \d+/)?.[0];
  if (!exactStats) throw new Error("frontier did not expose an exact combat allocation");
  const expectedSetup = await page.locator(".selected-build > span").innerText();
  await tradeoffs.getByRole("button", { name: "Use exact allocation", exact: true }).click();
  await expect(page.locator(".result-row-full")).toHaveCount(1);
  const exactRow = page.locator(".result-row-full").first();
  const exactWeapon = (await exactRow.locator(".weapon-cell strong").innerText()).trim();
  const exactAffinity = (await exactRow.locator(".setup-cell strong").innerText()).trim();
  const exactAow = (await exactRow.locator(".setup-cell > small").innerText()).trim();
  const exactUpgrade = (await exactRow.getByRole("gridcell").nth(3).innerText()).trim();
  const [expectedAffinity, expectedAow, expectedUpgrade] = expectedSetup.split(" / ");
  const exactAr = (await exactRow.locator(".ar-status-cell strong").innerText()).trim();
  const exactBleedLabel = await page.locator('[aria-label^="Bleed buildup:"]').first().getAttribute("aria-label");
  if (!exactBleedLabel) throw new Error("exact allocation did not expose bleed buildup");
  const returnedInspectorAr = (await page.locator(".metric-tile").filter({ hasText: "Max AR" }).locator("strong").innerText()).trim();
  const returnedBleed = Number(exactBleedLabel.match(/Bleed buildup:\s*([0-9.]+)/)?.[1]);
  if (returnedInspectorAr !== chosenPointAr) {
    throw new Error(`exact allocation AR ${returnedInspectorAr} did not match chosen point AR ${chosenPointAr}`);
  }
  if (!Number.isFinite(returnedBleed) || returnedBleed !== Number(chosenPointBleed)) {
    throw new Error(`exact allocation bleed ${returnedBleed} did not match chosen point bleed ${chosenPointBleed}`);
  }
  await expect(exactRow.locator(".weapon-cell strong")).toHaveText(selectedWeapon);
  await expect(exactRow.locator(".setup-cell strong")).toHaveText(expectedAffinity);
  await expect(exactRow.locator(".setup-cell > small")).toHaveText(expectedAow);
  await expect(exactRow.getByRole("gridcell").nth(3)).toHaveText(expectedUpgrade);
  await expect(exactRow.locator(".row-combat-stats")).toHaveText(exactStats);
  await expect(page.locator(".detail-block").filter({ hasText: "Combat Stats" }).locator("strong")).toHaveText(exactStats);
  if (!exactAr) throw new Error("exact allocation did not expose AR");
  markSmokeStage("save exact applied tradeoff");
  const presetName = `Release verification ${Date.now()}`;
  await page.getByRole("textbox", { name: "Name", exact: true }).fill(presetName);
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByText(`Saved ${presetName}.`, { exact: true }).waitFor();
  await page.getByRole("navigation").getByRole("button", { name: "Compare" }).click();
  await page.getByText("Comparison current", { exact: true }).waitFor();
  await page.getByRole("button", { name: "Compare Type", exact: true }).click();
  await page.getByRole("group", { name: "Compare Type", exact: true })
    .getByRole("checkbox", { name: /^Axe\b/ })
    .check();
  await page.keyboard.press("Escape");
  const bestTypeLane = page.locator(".compare-lane", { hasText: "Best Axe" });
  markSmokeStage("wait for best-Axe comparison");
  await bestTypeLane.locator("strong").waitFor();
  const bestTypeWeapon = (await bestTypeLane.locator("strong").textContent())?.trim();
  if (!bestTypeWeapon) throw new Error("best-Axe comparison did not resolve a target");
  await page.getByText("Comparison current", { exact: true }).waitFor();

  markSmokeStage("run Paths preview");
  await page.getByRole("navigation").getByRole("button", { name: "Paths" }).click();
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("10");
  await page.getByRole("button", { name: "Trace paths", exact: true }).click();
  markSmokeStage("wait for Paths preview");
  await page.getByRole("table", { name: "Path steps", exact: true }).locator('[role="row"]').nth(1).waitFor();

  markSmokeStage("run Affinity Watch");
  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch" }).click();
  await page.getByRole("spinbutton", { name: "Current + N" }).fill("10");
  await page.getByRole("button", { name: "Watch affinities", exact: true }).click();
  markSmokeStage("wait for Affinity Watch");
  await page.getByRole("grid", { name: "Affinity watch rankings" }).locator('[role="row"]').nth(1).waitFor();

  markSmokeStage("reload saved exact tradeoff");
  await page.reload();
  await page.getByRole("combobox", { name: "Saved", exact: true }).selectOption({ label: `${presetName} — vanilla · current data` });
  await page.getByRole("button", { name: "Load", exact: true }).click();
  markSmokeStage("wait for preset load");
  await page.getByText(`Loaded ${presetName}.`, { exact: true }).waitFor();
  await expect(page.locator(".result-row-full")).toHaveCount(1);
  const reloadedRow = page.locator(".result-row-full").first();
  await expect(reloadedRow.locator(".weapon-cell strong")).toHaveText(exactWeapon);
  await expect(reloadedRow.locator(".setup-cell strong")).toHaveText(exactAffinity);
  await expect(reloadedRow.locator(".setup-cell > small")).toHaveText(exactAow);
  await expect(reloadedRow.getByRole("gridcell").nth(3)).toHaveText(exactUpgrade);
  await expect(reloadedRow.locator(".row-combat-stats")).toHaveText(exactStats);
  await expect(reloadedRow.locator(".ar-status-cell strong")).toHaveText(exactAr);
  await expect(page.locator(`[aria-label="${exactBleedLabel}"]`)).toBeVisible();
  await expect(page.locator(".selected-build > strong")).toHaveText(exactWeapon);
  await expect(page.locator(".selected-build > span")).toHaveText(`${exactAffinity} / ${exactAow} / ${exactUpgrade}`);
  await expect(page.locator(".detail-block").filter({ hasText: "Combat Stats" }).locator("strong")).toHaveText(exactStats);
  markSmokeStage("save stale results as inputs only");
  const twoHanding = page.getByRole("checkbox", { name: "Two-handing", exact: true });
  await twoHanding.setChecked(!(await twoHanding.isChecked()));
  await page.getByText("Inputs changed", { exact: true }).waitFor();
  await page.getByRole("button", { name: "Update selected", exact: true }).click();
  await page.getByText(/Inputs only; rerun search for current results/).waitFor();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.locator(".result-row-full")).toHaveCount(0);
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  markSmokeStage("wait for preset deletion");
  await page.getByText(`Deleted ${presetName}.`, { exact: true }).waitFor();
  await page.reload();
  markSmokeStage("wait for final Vanilla model");
  await page.getByText("Snapshot loaded", { exact: true }).waitFor();
  const savedBuilds = page.getByRole("combobox", { name: "Saved", exact: true });
  await savedBuilds.locator('option[value=""]').waitFor({ state: "attached" });
  if (await savedBuilds.inputValue() !== "") {
    throw new Error("packaged smoke preset remained selected after deletion and reload");
  }
  if (await savedBuilds.locator(`option:has-text("${presetName}")`).count()) {
    throw new Error("packaged smoke preset survived explicit cleanup");
  }

  if (policyErrors.length > 0) throw new Error(`Unexpected CSP or IPC errors: ${policyErrors.join("\n")}`);
  if (ipcResponses === 0) throw new Error("Packaged smoke observed no custom-protocol IPC responses");
  process.stdout.write(`PACKAGED_SMOKE_PASSED ${JSON.stringify({ selectedWeapon, bestTypeWeapon, presetName, ipcResponses })}\n`);
} catch (error) {
  const output = session?.output().trim();
  const pageState = session?.page
    ? await session.page.locator(
      '.error-strip[role="alert"], .startup-state[role="alert"], .profile-coverage, .analysis-state, .progress-strip, .search-button',
    ).evaluateAll((elements) => elements.map((element) => ({
      className: element.className,
      role: element.getAttribute("role"),
      text: element.textContent?.replace(/\s+/g, " ").trim() ?? "",
      disabled: "disabled" in element ? element.disabled : undefined,
    }))).catch(() => [])
    : [];
  const suffix = [
    `\nPackaged smoke stage: ${smokeStage}`,
    pageState.length ? `\nPackaged page state:\n${JSON.stringify(pageState, null, 2)}` : "",
    output ? `\nPackaged app output:\n${output.slice(-4000)}` : "",
  ].join("");
  if (error instanceof Error) {
    const originalStack = error.stack;
    if (originalStack) {
      error.stack = `${originalStack}${suffix}`;
    } else {
      error.message = `${error.message}${suffix}`;
    }
    throw error;
  }
  throw new Error(`${String(error)}${suffix}`);
} finally {
  if (session?.page && previousCompareBench !== undefined) {
    await session.page.evaluate(({ key, value }) => {
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    }, { key: vanillaCompareBenchKey, value: previousCompareBench }).catch(() => {});
  }
  await stopSession(session);
}

async function assertProductionConnections(page) {
  const report = await page.evaluate(async () => {
    if (location.hostname !== "tauri.localhost") throw new Error(`Expected packaged origin, received ${location.origin}`);
    const response = await fetch(location.href);
    const csp = response.headers.get("content-security-policy");
    if (!csp) throw new Error("Packaged document did not expose an enforced CSP header");
    const blocked = [];
    for (const host of ["localhost", "127.0.0.1", "127.1", "[::1]"]) {
      for (const scheme of ["http", "https", "ws", "wss"]) {
        const target = `${scheme}://${host}:1420/__csp_probe__`;
        const violation = await new Promise((resolve) => {
          const controller = new AbortController();
          let socket;
          const finish = (value) => {
            clearTimeout(timer);
            document.removeEventListener("securitypolicyviolation", onViolation);
            controller.abort();
            socket?.close();
            resolve(value);
          };
          const onViolation = (event) => {
            if (event.effectiveDirective === "connect-src" && event.disposition === "enforce"
              && new URL(event.blockedURI).origin === new URL(target).origin) {
              finish({ target, blockedURI: event.blockedURI, directive: event.effectiveDirective, disposition: event.disposition });
            }
          };
          const timer = setTimeout(() => finish(null), 1_500);
          document.addEventListener("securitypolicyviolation", onViolation);
          if (scheme.startsWith("ws")) {
            try {
              socket = new WebSocket(target);
              socket.addEventListener("error", () => {});
            } catch {
              // Only an enforced CSP event establishes denial; a socket error does not.
            }
          } else {
            void fetch(target, { mode: "no-cors", signal: controller.signal }).catch(() => {});
          }
        });
        if (!violation) throw new Error(`No enforced connect-src violation for ${target}`);
        blocked.push(violation);
      }
    }
    return { origin: location.origin, csp, blocked };
  });
  process.stdout.write(`PACKAGED_SMOKE_CSP ${JSON.stringify(report)}\n`);
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
