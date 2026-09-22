import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { cpus, platform, release, arch } from "node:os";
import { cancellationMeasured, medianSample, sampleStatistics } from "./native-responsiveness-metrics.mjs";
import { launchPackagedApp, stopSession } from "./packaged-session.mjs";

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
const suite = options.suite ?? "analysis";
if (suite !== "analysis" && suite !== "production") throw new Error("--suite must be analysis or production");
if (suite === "production" && mode !== "async") throw new Error("The production suite requires asynchronous native commands");
const POLL_INTERVAL_MS = 25;

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

const executableSha256 = createHash("sha256").update(await readFile(executable)).digest("hex");
const rayonThreads = options.threads === "default"
  ? null : positiveInteger(options.threads ?? process.env.RAYON_NUM_THREADS, 1, "--threads/RAYON_NUM_THREADS");
if (rayonThreads === null) delete process.env.RAYON_NUM_THREADS;
else process.env.RAYON_NUM_THREADS = String(rayonThreads);
const appStartedAt = performance.now();
const session = await launchPackagedApp(executable);
try {
  const page = session.page;
  page.setDefaultTimeout(600_000);
  await page.getByText("Snapshot loaded", { exact: true }).waitFor({ timeout: 120_000 });
  const manifest = await invokeNative(page, "get_data_manifest", { profileId: "vanilla" });
  const startupMs = round(performance.now() - appStartedAt);

  const solved = mode === "async"
    ? await waitForJob(page, commandNames.solveStart, commandNames.analysisStatus, {
      request: solveRequest(BASE_REQUEST),
    })
    : await invokeNative(page, "solve_build", { request: solveRequest(BASE_REQUEST) });
  if (!solved) throw new Error("baseline fixture did not produce a solved Uchigatana row");

  const cases = suite === "production" ? await productionCases(page) : [
    ["uncached-solve-batch", () => runSolveBatch(page)],
    ["build-upgrade-series", () => runSeries(page, solved)],
    ...(mode === "async" ? [
      ["cancellation", () => runCancellation(page)],
      ["ar-bleed-frontier", async () => {
        const request = { base: BASE_REQUEST, solved };
        return { ...await measureNativeWork(page,
          () => waitForJob(page, "start_ar_bleed_frontier", commandNames.analysisStatus, { request }),
          "ar-bleed-frontier"), request };
      }],
      ["frontier-cancellation", () => measureCancellableJob(page, {
        start: "start_ar_bleed_frontier", status: commandNames.analysisStatus, cancel: commandNames.analysisCancel,
        request: { base: { ...BASE_REQUEST, characterLevel: 300, vig: 12, end: 13 }, solved },
        result: status => status.finished?.frontier, delayMs: 0,
      })],
    ] : []),
  ];
  const selectedCases = cases.filter(([name]) => options.case === undefined || options.case === name);
  if (selectedCases.length === 0) throw new Error(`Unknown case for ${suite}: ${options.case}`);
  const measurements = [];
  for (const [name, run] of selectedCases) {
    const warmupSamples = [];
    for (let index = 0; index < warmups; index += 1) warmupSamples.push(await run());
    const samples = [];
    for (let index = 0; index < repeats; index += 1) {
      const sample = await run();
      samples.push(sample);
      const { fingerprint: result, resultFingerprint, request, requests, ...timings } = sample;
      process.stdout.write(`NATIVE_RESPONSIVENESS_SAMPLE ${JSON.stringify({ name, index: index + 1, ...timings })}\n`);
    }
    const allSamples = [...warmupSamples, ...samples];
    const fingerprints = new Set(allSamples.map((sample) => sample.fingerprint ?? sample.resultFingerprint));
    const requests = new Set(allSamples.map((sample) => fingerprint(sample.request ?? sample.requests)));
    if (requests.size !== 1 || requests.has(undefined)) throw new Error(`${name} changed or omitted requests across warmups/repeats`);
    if (!name.endsWith("cancellation") && (fingerprints.size !== 1 || fingerprints.has(undefined))) {
      throw new Error(`${name} changed or omitted its result fingerprint across warmups/repeats`);
    }
    const median = medianSample(samples);
    measurements.push({ name, warmups, warmupSamples, samples, statistics: sampleStatistics(samples), ...(median ? { median } : {}) });
  }

  const report = {
    schemaVersion: 3,
    executable,
    executableSha256,
    environment: { node: process.version, platform: platform(), release: release(), arch: arch(), cpu: cpus()[0]?.model, logicalCpus: cpus().length,
      rayonThreads, rayonThreadPolicy: rayonThreads === null ? "runtime-default; RAYON_NUM_THREADS unset, native pool size not instrumented" : "explicit" },
    manifest,
    mode,
    suite,
    requestedCase: options.case ?? null,
    pollIntervalMs: POLL_INTERVAL_MS,
    timingScope: "Native job start through terminal status observed over IPC, including scheduling, serialization and polling; not core calculation time or frame latency",
    commandNames,
    warmups,
    repeats,
    startupMs,
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

async function productionCases(page) {
  const base = {
    ...BASE_REQUEST, characterLevel: 80, vig: 12, mnd: 11, end: 13,
    strStat: 12, dex: 15, intStat: 9, fai: 8, arc: 8,
    dlcScaling: false, scadutreeLevel: 0, topK: 1,
  };
  const coupledRequest = solveRequest({
    ...base, strStat: 20, lockStr: 20, lockInt: 9, lockFai: 8, objective: "aow_full_sequence",
  }, "Caestus", "Occult", "Lifesteal Fist");
  const coupled = await waitForJob(page, commandNames.solveStart, commandNames.analysisStatus, { request: coupledRequest });
  if (coupled?.aowRoute?.routeId !== "full" || coupled.stats.strStat !== 20 || coupled.stats.intStat !== 9 || coupled.stats.fai !== 8) {
    throw new Error("Locked Lifesteal Fist fixture did not preserve the full route and requested locks");
  }
  // Lifesteal Fist's six full-route rows use correction 52000's 70% STR/DEX
  // influence rates. The pinned model classifies that coupled route as non-additive.
  const fixtures = [];
  for (const [weapon, affinity, ash] of [
    ["Uchigatana", "Keen", "Unsheathe"],
    ["Bloodhound's Fang", "Standard", "Bloodhound's Finesse"],
  ]) {
    const request = solveRequest(base, weapon, affinity, ash);
    const solved = await waitForJob(page, commandNames.solveStart, commandNames.analysisStatus, { request });
    if (!solved) throw new Error(`Paths fixture ${weapon} produced no build`);
    fixtures.push({ base, solved, title: weapon });
  }
  if (fixtures[0].solved.weaponId === fixtures[1].solved.weaponId) throw new Error("Paths requires distinct loadout fixtures");
  const highK = { ...base, characterLevel: 46, topK: 500, resultGrouping: "loadout" };
  const cases = [
    ["locked-non-additive-solve", async () => ({
      ...await measureNativeWork(page, () => waitForJob(page, commandNames.solveStart, commandNames.analysisStatus, { request: coupledRequest }), "locked-non-additive-solve"),
      request: coupledRequest,
    })],
    ["search-high-k", async () => ({
      ...await measureNativeWork(page, async () => {
        const rows = await waitForJob(page, "start_search", "get_search_status", { request: highK }, finished => finished.rows);
        if (!Array.isArray(rows) || rows.length <= 25 || rows.length > highK.topK) throw new Error("High-K search did not produce a high-count result batch");
        return rows;
      }, "search-high-k"), request: highK,
    })],
  ];
  for (const pathMode of ["no_respec", "optimum_envelope"]) {
    const request = { requests: fixtures.map(fixture => ({ ...fixture, mode: pathMode, levelsAhead: 50 })) };
    cases.push([`paths-${pathMode}-50-two-lanes`, async () => ({
      ...await measureNativeWork(page, async () => {
        const paths = await waitForJob(page, "start_path_preview", "get_path_preview_status", { request }, finished => finished.paths);
        if (!Array.isArray(paths) || paths.length !== 2 || paths.some(path => path.steps.length !== 51 || path.steps[0].level !== 80 || path.steps.at(-1).level !== 130)) {
          throw new Error("Paths result did not cover both lanes and all requested levels");
        }
        return paths;
      }, `paths-${pathMode}`), request,
    })]);
  }
  cases.push(["search-cancellation", () => measureCancellableJob(page, {
    start: "start_search", status: "get_search_status", cancel: "cancel_search",
    request: { ...highK, characterLevel: 300, exactUpgrade: false }, result: status => status.finished?.rows, delayMs: 0,
  })]);
  cases.push(["analysis-cancellation", () => runCancellation(page)]);
  for (const pathMode of ["no_respec", "optimum_envelope"]) {
    cases.push([`paths-${pathMode}-cancellation`, () => measureCancellableJob(page, {
      start: "start_path_preview", status: "get_path_preview_status", cancel: "cancel_path_preview",
      request: { requests: [
        { base: coupledRequest.base, solved: coupled, title: "Caestus / Lifesteal Fist", mode: pathMode, levelsAhead: 200 },
        { ...fixtures[1], mode: pathMode, levelsAhead: 200 },
      ] }, result: status => status.finished?.paths, delayMs: 0,
    })]);
  }
  return cases;
}

async function runSolveBatch(page) {
  const requests = Array.from({ length: 8 }, () => solveRequest({
    ...BASE_REQUEST,
    exactUpgrade: false,
    topK: 1,
  }));
  return { ...await measureNativeWork(page, () => runHeavyBatch(page, "solve_build", requests), "solve_build"), requests };
}

async function runHeavyBatch(page, command, requests) {
  const results = [];
  for (const request of requests) results.push(await invokeHeavy(page, command, { request }));
  return results;
}

async function runSeries(page, solved) {
  const request = seriesRequest({ ...BASE_REQUEST }, solved);
  return { ...await measureNativeWork(page, () => invokeHeavy(page, "build_upgrade_series", { request }), "build_upgrade_series"), request };
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
  const delayMs = config.delayMs ?? 20;
  if (delayMs > 0) await delay(delayMs);
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
  const measured = cancellationMeasured(cancelAccepted, finished?.finished);
  if (!measured) {
    process.stderr.write("Cancellation timing inconclusive: calculation finished before cancellation took effect.\n");
  }
  return {
    request: config.request,
    operation: `${config.start}/${config.cancel}`,
    startMs: round(cancelRequestedAt - startedAt),
    cancelMs: round(finishedAt - cancelRequestedAt),
    lightMs: round(lightFinishedAt - lightStartedAt),
    totalMs: round(finishedAt - startedAt),
    cancelAccepted,
    cancelled: finished?.finished?.cancelled === true,
    cancellationMeasured: measured,
    requestedDelayMs: delayMs,
    resultFingerprint: fingerprint(config.result?.(finished) ?? finished?.finished),
    lightCommand: "get_data_manifest",
    lightCompletedBeforeHeavy: lightFinishedAt <= finishedAt,
  };
}

async function waitForJob(page, startCommand, statusCommand, args, resultOf) {
  const started = await invokeNative(page, startCommand, args);
  const jobId = started?.jobId;
  if (typeof jobId !== "string") throw new Error(`${startCommand} did not return jobId`);
  const status = await waitForJobStatus(page, statusCommand, jobId);
  const finished = status?.finished;
  if (!finished) throw new Error(`${startCommand} returned no finished status`);
  if (finished.cancelled) throw new Error(`${startCommand} unexpectedly cancelled`);
  if (finished.error) throw new Error(`${startCommand} failed: ${finished.error}`);
  if (resultOf) return resultOf(finished);
  if (finished.kind === "solve_build") return finished.result;
  if (finished.kind === "upgrade_series") return finished.points;
  if (finished.kind === "ar_bleed_frontier") return finished.frontier;
  throw new Error(`${startCommand} returned unknown analysis kind`);
}

async function waitForJobStatus(page, command, jobId) {
  const deadline = Date.now() + 600_000;
  while (Date.now() < deadline) {
    const status = await invokeNative(page, command, { jobId });
    if (status?.finished) return status;
    await delay(POLL_INTERVAL_MS);
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

function round(value) {
  return Math.round(value * 100) / 100;
}

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

function parseOptions(args) {
  const parsed = {};
  const allowed = new Set(["warmups", "repeats", "mode", "suite", "threads", "case", "output"]);
  for (const arg of args) {
    const match = /^--([^=]+)(?:=(.*))?$/.exec(arg);
    if (!match || !allowed.has(match[1]) || Object.hasOwn(parsed, match[1])) throw new Error(`unknown or duplicate argument ${arg}`);
    parsed[match[1]] = match[2] ?? "true";
  }
  return parsed;
}

function positiveInteger(value, fallback, name) {
  const parsed = value === undefined ? fallback : Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}
