import { chromium } from "@playwright/test";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { createServer } from "node:net";
import { randomUUID } from "node:crypto";
import { join } from "node:path";

const START_ATTEMPTS = 2;
const STARTUP_TIMEOUT_MS = 90_000;
const RETRY_COOLDOWN_MS = 5_000;

// Ctrl+C or a cancelled CI job must not leave the app, its WebView2 helpers or their
// profile directory behind.
const activeSessions = new Set();
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.once(signal, async () => {
    await Promise.all([...activeSessions].map(stopSession));
    process.exit(130);
  });
}

export async function launchPackagedApp(executablePath) {
  const failures = [];
  for (let attempt = 1; attempt <= START_ATTEMPTS; attempt += 1) {
    const localAppData = process.env.LOCALAPPDATA;
    if (!localAppData) throw new Error("LOCALAPPDATA is required for packaged smoke isolation");
    const profileToken = `tarnisheds-arsenal-smoke-${randomUUID()}`;
    const profileDirectory = join(localAppData, "main", profileToken);
    if (existsSync(profileDirectory)) throw new Error("packaged smoke profile directory already exists");
    const port = await reserveLoopbackPort();
    const endpoint = `http://127.0.0.1:${port}`;
    let output = "";
    /** @type {{ code: number | null, signal: NodeJS.Signals | null } | null} */
    let exit = null;
    const startedAt = performance.now();
    const child = spawn(executablePath, [], {
      // The app opens the debugging port only for this variable and only with an isolated
      // smoke-only WebView2 data directory, which every attempt gets fresh.
      env: {
        ...process.env,
        TARNISHEDS_ARSENAL_SMOKE_DEBUG_PORT: String(port),
        WEBVIEW2_USER_DATA_FOLDER: profileDirectory,
        // A failed start then exits with its error on stderr instead of a modal message box.
        TARNISHEDS_ARSENAL_SMOKE_NO_DIALOG: "1",
      },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    process.stdout.write(
      `PACKAGED_SMOKE_START attempt=${attempt}/${START_ATTEMPTS} pid=${child.pid ?? "unknown"} port=${port} timeoutMs=${STARTUP_TIMEOUT_MS}\n`,
    );
    child.stdout.on("data", (chunk) => { output += chunk.toString(); });
    child.stderr.on("data", (chunk) => { output += chunk.toString(); });
    child.once("exit", (code, signal) => { exit = { code, signal }; });
    // A spawn that failed may never emit "exit"; record it so startup stops waiting.
    child.once("error", (error) => {
      output += `spawn failed: ${error.message}\n`;
      exit ??= { code: null, signal: null };
    });
    const candidate = {
      browser: undefined,
      child,
      exit: () => exit,
      output: () => output,
      page: undefined,
      profileDirectory,
      startedAt,
    };
    activeSessions.add(candidate);

    try {
      await waitForEndpoint(`${endpoint}/json/version`, STARTUP_TIMEOUT_MS, () => exit);
      assertEndpointOwner(port, child.pid);
      candidate.browser = await chromium.connectOverCDP(endpoint);
      candidate.page = await waitForAppPage(candidate.browser, 30_000, () => exit);
      if (!existsSync(profileDirectory)) throw new Error("WebView2 did not create the isolated smoke profile");
      return candidate;
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      const childState = exit
        ? `exited with code ${String(exit.code)} and signal ${String(exit.signal)}`
        : `still running as PID ${child.pid ?? "unknown"}`;
      const captured = output.trim();
      failures.push(
        `attempt ${attempt}/${START_ATTEMPTS} on ${endpoint}: ${reason}; process ${childState}`
        + (captured ? `\n${captured.slice(-2000)}` : ""),
      );
      await stopSession(candidate);
      // Only a WebView2 that never came up is retried; an app that exited is a real failure.
      if (exit || attempt === START_ATTEMPTS) break;
      process.stdout.write(`::warning::Packaged app startup was retried: ${reason}\n`);
      process.stdout.write(`PACKAGED_SMOKE_RETRY cooldownMs=${RETRY_COOLDOWN_MS}\n`);
      await new Promise((resolve) => setTimeout(resolve, RETRY_COOLDOWN_MS));
    }
  }
  throw new Error(`packaged WebView2 startup failed:\n${failures.join("\n")}`);
}

export async function stopSession(sessionToStop) {
  if (!sessionToStop) return;
  activeSessions.delete(sessionToStop);
  await sessionToStop.browser?.close().catch(() => undefined);
  // The app's WebView2 helpers are child processes and hold the profile directory open.
  if (!sessionToStop.exit()) spawnSync("taskkill", ["/PID", String(sessionToStop.child.pid), "/T", "/F"], { stdio: "ignore" });
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
    maxRetries: 10,
    retryDelay: 250,
  }).catch((error) => {
    process.stderr.write(`PACKAGED_SMOKE_CLEANUP could not remove ${sessionToStop.profileDirectory}: ${error.message}\n`);
  });
}

async function reserveLoopbackPort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(undefined));
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    server.close();
    throw new Error("could not reserve a loopback port for packaged smoke testing");
  }
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  return address.port;
}

// The port is released before the app binds it, so another local process could take it and
// pose as the debugger. Accept only a listener that is the spawned app or its WebView2 child.
function assertEndpointOwner(port, appPid) {
  const script = `$owner = (Get-NetTCPConnection -LocalAddress 127.0.0.1 -LocalPort ${port} -State Listen -ErrorAction Stop | Select-Object -First 1).OwningProcess; `
    + "Write-Output $owner (Get-CimInstance Win32_Process -Filter ('ProcessId=' + $owner)).ParentProcessId";
  const [owner, parent] = execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], { encoding: "utf8" })
    .trim().split(/\s+/).map(Number);
  if (owner !== appPid && parent !== appPid) {
    throw new Error(`debugging port ${port} belongs to PID ${owner} (parent ${parent}), not the packaged app ${appPid}`);
  }
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
