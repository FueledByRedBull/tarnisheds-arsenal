import { spawn } from "node:child_process";
import { createServer } from "vite";

const host = "127.0.0.1";

let server = null;

try {
  server = await createServer({
    logLevel: "error",
    server: { host, port: 0 },
  });
  await server.listen();
  const baseUrl = server.resolvedUrls?.local[0];
  if (!baseUrl) {
    throw new Error("Vite did not report a local test URL.");
  }

  const code = await runPlaywright(baseUrl);
  process.exitCode = code;
} finally {
  if (server) {
    await server.close();
  }
}

function runPlaywright(baseUrl) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["./node_modules/@playwright/test/cli.js", "test"], {
      env: { ...process.env, PLAYWRIGHT_BASE_URL: baseUrl, PW_TEST_HTML_REPORT_OPEN: "never" },
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
