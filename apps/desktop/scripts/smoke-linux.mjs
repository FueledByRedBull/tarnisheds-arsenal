import { spawn } from "node:child_process";
import { existsSync } from "node:fs";

// Playwright reaches the Windows app through WebView2's debugging port, which WebKitGTK does
// not have, so this drives an installed Linux build over WebDriver with no extra packages.
const application = process.argv[2];
if (!application || !existsSync(application)) {
  throw new Error("Usage: node scripts/smoke-linux.mjs <application-binary>");
}

const DRIVER_URL = "http://127.0.0.1:4444";
const ELEMENT_KEY = "element-6066-11e4-a52e-4f735466cecf";
const TIMEOUT_MS = 120_000;
const driver = spawn("tauri-driver", [], { stdio: ["ignore", "inherit", "inherit"] });

async function request(method, path, body) {
  const response = await fetch(`${DRIVER_URL}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const { value } = await response.json();
  if (!response.ok) throw new Error(`${method} ${path}: ${JSON.stringify(value)}`);
  return value;
}

async function waitFor(description, read) {
  const deadline = Date.now() + TIMEOUT_MS;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const value = await read();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timed out waiting for ${description}: ${String(lastError ?? "no match")}`);
}

let sessionId;
try {
  await waitFor("tauri-driver", async () => (await fetch(`${DRIVER_URL}/status`)).ok);
  ({ sessionId } = await request("POST", "/session", {
    capabilities: { alwaysMatch: { browserName: "wry", "tauri:options": { application } } },
  }));
  const session = `/session/${sessionId}`;
  const find = async (using, value) => (await request("POST", `${session}/elements`, { using, value })).map((element) => element[ELEMENT_KEY]);
  const text = (element) => request("GET", `${session}/element/${element}/text`);
  const click = async (xpath, description) => {
    const [element] = await waitFor(description, async () => {
      const elements = await find("xpath", xpath);
      return elements.length ? elements : null;
    });
    await request("POST", `${session}/element/${element}/click`, {});
  };
  const waitForText = (wanted) => waitFor(`"${wanted}"`, async () => (await find("xpath", `//*[normalize-space()='${wanted}']`)).length > 0);
  const search = async (profile) => {
    await click("//button[normalize-space()='Search']", "the Search button");
    const [row] = await waitFor(`${profile} results`, async () => {
      const rows = await find("css selector", ".result-row");
      return rows.length ? rows : null;
    });
    const summary = await text(row);
    if (!/\d/.test(summary) || !summary.includes("1st hit")) {
      throw new Error(`${profile} result has no attack power and skill damage: ${summary}`);
    }
    process.stdout.write(`LINUX_SMOKE_RESULT ${profile} ${JSON.stringify(summary.split("\n").slice(0, 4))}\n`);
  };

  await waitForText("Snapshot loaded");
  await search("vanilla");
  await click("//*[@role='radio'][contains(., 'Convergence')]", "the Convergence profile");
  await waitForText("Experimental model");
  await search("convergence");
  process.stdout.write("LINUX_SMOKE_PASSED\n");
} finally {
  if (sessionId) await request("DELETE", `/session/${sessionId}`).catch(() => {});
  driver.kill();
}
