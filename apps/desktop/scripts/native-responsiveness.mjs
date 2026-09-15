import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { existsSync } from "node:fs";
import { rm, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { join, resolve } from "node:path";

const [executableArg, ...rawOptions] = process.argv.slice(2);
if (!executableArg || !existsSync(executableArg)) {
  throw new Error("Usage: node scripts/native-responsiveness.mjs <packaged-executable> [options]");
}

const options = parseOptions(rawOptions);
const executable = resolve(executableArg);
const warmups = positiveInteger(options.warmups, 1, "--warmups");
const repeats = positiveInteger(options.repeats, 3, "--repeats");
const mode = options.mode ?? "async";
if (mode !== "sync" && mode !== "async") throw new Error("--mode must be sync or async");

const BASE_REQUEST = {
  profileId: "vanilla",
  className: "Samurai",
  characterLevel: 150,
  vig: 50,
  mnd: 11,
  end: 30,
  strStat: 12,
  dex: 60,
  intStat: 9,
  fai: 8,
  arc: 45,
  minStr: 12,
  minDex: 15,
  minInt: 9,
  minFai: 8,
  minArc: 8,
  lockStr: null,
  lockDex: null,
  lockInt: null,
  lockFai: null,
  lockArc: null,
  standardMaxUpgrade: 25,
  somberMaxUpgrade: 10,
  exactUpgrade: true,
  maxUpgrade: null,
  fixedUpgrade: null,
  twoHanding: false,
  dlcScaling: true,
  scadutreeLevel: 20,
  weaponName: null,
  affinity: null,
  aowName: null,
  weaponTypeKey: null,
  somberFilter: "all",
  filters: { version: 1, entries: [] },
  resultGrouping: "automatic",
  objective: "max_ar",
  topK: 10,
};

const commandNames = {
  solveStart: "start_solve_build",
  seriesStart: "start_upgrade_series",
  analysisStatus: "get_analysis_status",
  analysisCancel: "cancel_analysis",
};

const appStartedAt = Date.now();
const session = await launchPackagedApp(executable);
try {
  const page = session.page;
  page.setDefaultTimeout(600_000);
  await page.getByText("Full model ready", { exact: true }).waitFor({ timeout: 120_000 });
  await invokeNative(page, "get_data_manifest", { profileId: "vanilla" });

  const solved = mode === "async"
    ? await waitForJob(page, commandNames.solveStart, commandNames.analysisStatus, {
      request: solveRequest(BASE_REQUEST),
    })
    : await invokeNative(page, "solve_build", { request: solveRequest(BASE_REQUEST) });
  if (!solved) throw new Error("baseline fixture did not produce a solved Uchigatana row");

  const cases = [
    ["cold-multipin", () => runMultipin(page)],
    ["migration", () => runMigration(page)],
    ["build-upgrade-series", () => runSeries(page, solved)],
    ...(mode === "async" ? [["cancellation", () => runCancellation(page)]] : []),
  ];
  const measurements = [];
  for (const [name, run] of cases) {
    for (let index = 0; index < warmups; index += 1) await run();
    const samples = [];
    for (let index = 0; index < repeats; index += 1) {
      const sample = await run();
      samples.push(sample);
      const { fingerprint: result, resultFingerprint, ...timings } = sample;
      process.stdout.write(`NATIVE_RESPONSIVENESS_SAMPLE ${JSON.stringify({ name, index: index + 1, ...timings })}\n`);
    }
    const fingerprints = new Set(samples.map((sample) => sample.fingerprint ?? sample.resultFingerprint));
    if (name !== "cancellation" && fingerprints.size > 1) throw new Error(`${name} changed its result fingerprint across repeats`);
    measurements.push({ name, warmups, samples, median: medianSample(samples) });
  }

  const report = {
    schemaVersion: 1,
    executable,
    mode,
    commandNames,
    warmups,
    repeats,
    startupMs: Date.now() - appStartedAt,
    fixture: {
      profileId: BASE_REQUEST.profileId,
      weaponName: solved.weaponName,
      affinity: solved.affinity,
      requestFingerprint: JSON.stringify(BASE_REQUEST),
    },
    measurements,
  };
  process.stdout.write(`NATIVE_RESPONSIVENESS_SUMMARY ${JSON.stringify(measurements.map(({ name, median }) => ({ name, median })))}\n`);
  if (options.output) await writeFile(resolve(options.output), `${JSON.stringify(report, null, 2)}\n`, "utf8");
} finally {
  await stopSession(session);
}

function solveRequest(base, weaponName = "Uchigatana", affinity = "Blood", aowName = "Seppuku") {
  return { base, weaponName, affinity, aowName };
}

function seriesRequest(base, solved) {
  return { base, solved, maxUpgrade: solved.isSomber ? 10 : 25 };
}

async function runMultipin(page) {
  const requests = Array.from({ length: 8 }, () => solveRequest({
    ...BASE_REQUEST,
    exactUpgrade: false,
    topK: 1,
  }));
  return measureNativeWork(page, () => runHeavyBatch(page, "solve_build", requests), "solve_build");
}

async function runMigration(page) {
  const requests = Array.from({ length: 8 }, () => solveRequest({
    ...BASE_REQUEST,
    exactUpgrade: false,
    topK: 1,
  }));
  return measureNativeWork(page, () => runHeavyBatch(page, "solve_build", requests), "migration.solve_build");
}

async function runHeavyBatch(page, command, requests) {
  const results = [];
  for (const request of requests) results.push(await invokeHeavy(page, command, { request }));
  return results;
}

async function runSeries(page, solved) {
  return measureNativeWork(
    page,
    () => invokeHeavy(page, "build_upgrade_series", { request: seriesRequest({ ...BASE_REQUEST }, solved) }),
    "build_upgrade_series",
  );
}

async function runCancellation(page) {
  if (mode === "async") {
    const cancellationBase = {
      ...BASE_REQUEST,
      characterLevel: 300,
      vig: 12,
      mnd: 11,
      end: 13,
      strStat: 12,
      dex: 15,
      intStat: 9,
      fai: 8,
      arc: 8,
    };
    return measureCancellableJob(page, {
      start: commandNames.solveStart,
      status: commandNames.analysisStatus,
      cancel: commandNames.analysisCancel,
      request: solveRequest(
        { ...cancellationBase, exactUpgrade: false, topK: 1 },
        "Sword of Night and Flame",
        "Standard",
        "Night-and-Flame Stance",
      ),
      result: (status) => status.finished?.result ?? null,
      delayMs: 0,
    });
  }
  throw new Error("Direct calculation cancellation is unavailable in the baseline binary");
}

async function measureNativeWork(page, heavy, label) {
  const heavyStartedAt = performance.now();
  let heavyFinishedAt;
  const heavyPromise = Promise.resolve(heavy()).then((result) => {
    heavyFinishedAt = performance.now();
    return result;
  });
  await delay(1);
  const lightStartedAt = performance.now();
  let lightFinishedAt;
  const lightPromise = invokeNative(page, "get_data_manifest", { profileId: "vanilla" }).then((result) => {
    lightFinishedAt = performance.now();
    return result;
  });
  const [heavyResult, lightResult] = await Promise.all([heavyPromise, lightPromise]);
  const finishedAt = performance.now();
  return {
    operation: label,
    heavyMs: round(heavyFinishedAt - heavyStartedAt),
    lightMs: round(lightFinishedAt - lightStartedAt),
    totalMs: round(finishedAt - heavyStartedAt),
    fingerprint: fingerprint(heavyResult),
    resultCount: Array.isArray(heavyResult) ? heavyResult.length : undefined,
    lightCommand: "get_data_manifest",
    lightCompleted: Boolean(lightResult),
    lightCompletedBeforeHeavy: lightFinishedAt <= heavyFinishedAt,
  };
}

async function invokeHeavy(page, command, args) {
  if (mode === "sync") return invokeNative(page, command, args);
  const config = command === "solve_build"
    ? { start: commandNames.solveStart, status: commandNames.analysisStatus, cancel: commandNames.analysisCancel }
    : { start: commandNames.seriesStart, status: commandNames.analysisStatus, cancel: commandNames.analysisCancel };
  return await waitForJob(page, config.start, config.status, args);
}

async function measureCancellableJob(page, config) {
  const startedAt = performance.now();
  const started = await invokeNative(page, config.start, { request: config.request });
  const jobId = started?.jobId;
  if (typeof jobId !== "string") throw new Error(`${config.start} did not return jobId`);
  await delay(config.delayMs ?? 20);
  const lightStartedAt = performance.now();
  let lightFinishedAt;
  const lightPromise = invokeNative(page, "get_data_manifest", { profileId: "vanilla" }).then((result) => {
    lightFinishedAt = performance.now();
    return result;
  });
  const cancelRequestedAt = performance.now();
  const cancelAccepted = await invokeNative(page, config.cancel, { jobId });
  const finished = await waitForJobStatus(page, config.status, jobId);
  const finishedAt = performance.now();
  await lightPromise;
  if (finished?.finished?.cancelled !== true) {
    process.stderr.write("Cancellation timing inconclusive: calculation finished before cancellation took effect.\n");
  }
  return {
    operation: `${config.start}/${config.cancel}`,
    startMs: round(cancelRequestedAt - startedAt),
    cancelMs: round(finishedAt - cancelRequestedAt),
    lightMs: round(lightFinishedAt - lightStartedAt),
    totalMs: round(finishedAt - startedAt),
    cancelAccepted,
    cancelled: finished?.finished?.cancelled === true,
    cancellationMeasured: finished?.finished?.cancelled === true,
    resultFingerprint: fingerprint(config.result?.(finished) ?? finished?.finished),
    lightCommand: "get_data_manifest",
    lightCompletedBeforeHeavy: lightFinishedAt <= finishedAt,
  };
}

async function waitForJob(page, startCommand, statusCommand, args) {
  const started = await invokeNative(page, startCommand, args);
  const jobId = started?.jobId;
  if (typeof jobId !== "string") throw new Error(`${startCommand} did not return jobId`);
  const status = await waitForJobStatus(page, statusCommand, jobId);
  const finished = status?.finished;
  if (!finished) throw new Error(`${startCommand} returned no finished status`);
  if (finished.cancelled) throw new Error(`${startCommand} unexpectedly cancelled`);
  if (finished.error) throw new Error(`${startCommand} failed: ${finished.error}`);
  if (finished.kind === "solve_build") return finished.result;
  if (finished.kind === "upgrade_series") return finished.points;
  throw new Error(`${startCommand} returned unknown analysis kind`);
}

async function waitForJobStatus(page, command, jobId) {
  const deadline = Date.now() + 600_000;
  while (Date.now() < deadline) {
    const status = await invokeNative(page, command, { jobId });
    if (status?.finished) return status;
    await delay(25);
  }
  throw new Error(`${command} timed out for ${jobId}`);
}

async function invokeNative(page, command, args) {
  return page.evaluate(async ({ command: name, args: commandArgs }) => {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== "function") throw new Error("Tauri internals invoke bridge is unavailable");
    try { return await internals.invoke(name, commandArgs); }
    catch (error) { throw new Error(JSON.stringify(error)); }
  }, { command, args });
}

function fingerprint(value) {
  return JSON.stringify(value);
}

function medianSample(samples) {
  const fields = ["heavyMs", "lightMs", "cancelMs", "totalMs"];
  return Object.fromEntries(fields
    .filter((field) => samples.some((sample) => Number.isFinite(sample[field])))
    .map((field) => [field, median(samples.map((sample) => sample[field]).filter(Number.isFinite))]));
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

function round(value) {
  return Math.round(value * 100) / 100;
}

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

function parseOptions(args) {
  const parsed = {};
  for (const arg of args) {
    const match = /^--([^=]+)(?:=(.*))?$/.exec(arg);
    if (!match) throw new Error(`unknown argument ${arg}`);
    parsed[match[1]] = match[2] ?? "true";
  }
  return parsed;
}

function positiveInteger(value, fallback, name) {
  const parsed = value === undefined ? fallback : Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

async function launchPackagedApp(executablePath) {
  const localAppData = process.env.LOCALAPPDATA;
  if (!localAppData) throw new Error("LOCALAPPDATA is required for packaged smoke isolation");
  const profileToken = `tarnisheds-arsenal-smoke-${randomUUID()}`;
  const profileDirectory = join(localAppData, "main", profileToken);
  if (existsSync(profileDirectory)) throw new Error("native responsiveness profile directory already exists");
  const port = await reserveLoopbackPort();
  const endpoint = `http://127.0.0.1:${port}`;
  let output = "";
  let exit = null;
  const child = spawn(executablePath, [
    `--packaged-smoke-port=${port}`,
    `--packaged-smoke-profile=${profileToken}`,
  ], {
    env: { ...process.env, WEBVIEW2_USER_DATA_FOLDER: profileDirectory },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  child.stdout.on("data", (chunk) => { output += chunk.toString(); });
  child.stderr.on("data", (chunk) => { output += chunk.toString(); });
  child.once("exit", (code, signal) => { exit = { code, signal }; });
  const session = {
    child,
    exit: () => exit,
    output: () => output,
    browser: null,
    page: null,
    profileDirectory,
  };
  try {
    await waitForEndpoint(`${endpoint}/json/version`, 120_000, () => exit);
    session.browser = await chromium.connectOverCDP(endpoint);
    session.page = await waitForAppPage(session.browser, 30_000, () => exit);
    return session;
  } catch (error) {
    await stopSession(session);
    throw new Error(`${error instanceof Error ? error.message : String(error)}\n${output.slice(-4000)}`);
  }
}

async function stopSession(sessionToStop) {
  if (!sessionToStop) return;
  await sessionToStop.browser?.close().catch(() => undefined);
  if (!sessionToStop.exit() && !sessionToStop.child.killed) sessionToStop.child.kill();
  await Promise.race([
    new Promise((resolveExit) => {
      if (sessionToStop.exit()) resolveExit();
      else sessionToStop.child.once("exit", resolveExit);
    }),
    delay(5_000),
  ]);
  if (!sessionToStop.exit()) sessionToStop.child.kill("SIGKILL");
  await rm(sessionToStop.profileDirectory, { recursive: true, force: true, maxRetries: 3, retryDelay: 250 }).catch(() => undefined);
}

async function reserveLoopbackPort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("could not reserve a loopback port");
  const port = address.port;
  await new Promise((resolveClose, reject) => server.close((error) => error ? reject(error) : resolveClose()));
  return port;
}

async function waitForEndpoint(url, timeoutMs, getExit) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (getExit()) throw new Error("packaged app exited before WebView2 was ready");
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(2_000) });
      if (response.ok && typeof (await response.json()).webSocketDebuggerUrl === "string") return;
    } catch {
      // WebView2 has not opened its local debugging endpoint yet.
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for packaged WebView2 endpoint ${url}`);
}

async function waitForAppPage(browser, timeoutMs, getExit) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (getExit()) throw new Error("packaged app exited before its page was ready");
    for (const context of browser.contexts()) {
      for (const page of context.pages()) if (await page.locator(".desktop-shell").count()) return page;
    }
    await delay(100);
  }
  throw new Error("packaged WebView2 page did not expose the application shell");
}
