import { spawn } from "node:child_process";
import { createServer } from "vite";

const host = "127.0.0.1";
const port = 1420;
const baseUrl = `http://${host}:${port}`;

let server = null;

try {
  if (!(await isAvailable(baseUrl))) {
    server = await createServer({
      logLevel: "error",
      server: { host, port, strictPort: true },
    });
    await server.listen();
    await waitForServer(baseUrl);
  }

  const code = await runPlaywright();
  process.exitCode = code;
} finally {
  if (server) {
    await server.close();
  }
}

async function isAvailable(url) {
  try {
    const response = await fetch(url);
    return response.ok;
  } catch {
    return false;
  }
}

async function waitForServer(url) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (await isAvailable(url)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

function runPlaywright() {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["./node_modules/@playwright/test/cli.js", "test"], {
      env: { ...process.env, PW_TEST_HTML_REPORT_OPEN: "never" },
      shell: false,
      stdio: "inherit",
    });

    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (signal) {
        reject(new Error(`Playwright exited with signal ${signal}`));
        return;
      }
      resolve(code ?? 1);
    });
  });
}
