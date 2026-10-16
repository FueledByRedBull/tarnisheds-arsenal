import { defineConfig } from "@playwright/test";

// scripts/run-e2e.mjs starts Vite on a free port and passes its URL; a default port could
// silently test whatever else is listening there.
const baseURL = process.env.PLAYWRIGHT_BASE_URL;
if (!baseURL) throw new Error("Run the browser tests with `npm run test:e2e`, which sets PLAYWRIGHT_BASE_URL.");

export default defineConfig({
  testDir: "./tests",
  forbidOnly: !!process.env.CI,
  use: {
    baseURL,
    trace: "retain-on-failure",
  },
});
