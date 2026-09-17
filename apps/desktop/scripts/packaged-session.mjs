import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { createServer } from "node:net";
import { randomUUID } from "node:crypto";
import { join } from "node:path";

export async function launchPackagedApp(executablePath, attempts = 2, timeoutMs = 90_000, cooldownMs = 5_000) {
  const failures = [];
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    const localAppData = process.env.LOCALAPPDATA;
    if (!localAppData) throw new Error("LOCALAPPDATA is required for packaged smoke isolation");
    const profileToken = `tarnisheds-arsenal-smoke-${randomUUID()}`;
    const profileDirectory = join(localAppData, "main", profileToken);
    if (existsSync(profileDirectory)) throw new Error("packaged smoke profile directory already exists");
    const port = await reserveLoopbackPort();
    const endpoint = `http://127.0.0.1:${port}`;
    let output = "";
    let exit = null;
    let browser;
    const child = spawn(executablePath, [
      `--packaged-smoke-port=${port}`,
      `--packaged-smoke-profile=${profileToken}`,
    ], {
      // Tauri 2.11.1 drops WindowConfig.data_directory while converting it to
      // WebviewAttributes. WebView2 honors this documented process override,
      // so every smoke attempt gets an isolated browser data directory.
      env: { ...process.env, WEBVIEW2_USER_DATA_FOLDER: profileDirectory },
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
      if (!existsSync(profileDirectory)) throw new Error("WebView2 did not create the isolated smoke profile");
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

export async function stopSession(sessionToStop) {
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

