import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e",
  use: { baseURL: "http://localhost:1420", viewport: { width: 1320, height: 800 } },
  webServer: { command: "pnpm dev", port: 1420, reuseExistingServer: true },
});
