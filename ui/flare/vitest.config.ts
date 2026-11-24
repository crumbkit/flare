import { defineConfig, mergeConfig } from "vitest/config";
import vite from "./vite.config.ts";

// Unit tests run in jsdom. The acceptance journey runs in a real browser
// through Playwright and is left out here.
export default mergeConfig(
  vite,
  defineConfig({
    test: {
      environment: "jsdom",
      include: ["src/**/*.test.ts"],
      restoreMocks: true,
      unstubGlobals: true,
      // A failed test is never retried (BASE-12).
      retry: 0,
      testTimeout: 30_000,
    },
  }),
);
