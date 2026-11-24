import { defineConfig, devices } from "@playwright/test";

// The acceptance journey drives the reading page in a real browser. A
// flare-server test writes artboard 01's sample share as a reader sees it
// (the page, its policy and the stylesheet) into FLARE_READING_SAMPLE; the
// journey serves that page and the built files in dist/assets from one
// origin, so the page runs under the policy flare-server sends. It runs in
// light, in dark and at 390 px wide.
export default defineConfig({
  testDir: "tests/acceptance",
  forbidOnly: true,
  // A failed test is never retried (BASE-12).
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: { trace: "retain-on-failure" },
  projects: [
    { name: "light", use: { ...devices["Desktop Chrome"], colorScheme: "light" } },
    { name: "dark", use: { ...devices["Desktop Chrome"], colorScheme: "dark" } },
    {
      name: "phone",
      use: {
        ...devices["Desktop Chrome"],
        colorScheme: "light",
        viewport: { width: 390, height: 844 },
      },
    },
  ],
});
