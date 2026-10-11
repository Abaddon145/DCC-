import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Bound simultaneous jsdom instances on desktop machines; avoid resource-dependent timing failures.
    maxWorkers: 2,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"]
  }
});
