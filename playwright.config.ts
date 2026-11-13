import { existsSync } from "node:fs";
import { defineConfig } from "@playwright/test";

// The pre-installed Chromium in this environment lives outside Playwright's default cache.
const preinstalled = "/opt/pw-browsers/chromium";

export default defineConfig({
  testDir: "e2e",
  timeout: 20_000,
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:1420",
    trace: "off",
    launchOptions: existsSync(preinstalled) ? { executablePath: preinstalled, args: ["--no-sandbox"] } : {},
  },
  // The UI runs in a plain browser against a mocked Tauri IPC layer (src/mocks). This validates
  // the React ⇄ typed-contract behaviour; it does NOT exercise the native shell (see docs).
  webServer: {
    command: "npx vite --mode mock",
    env: { VITE_MOCK_TAURI: "1" },
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
