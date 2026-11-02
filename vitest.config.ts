import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Bound simultaneous jsdom instances on desktop machines; avoid resource-dependent timing failures.
    // Windows Actions shares two workers and can spend the default 5s test budget inside the first App
    // render, or miss a control that appears after a short debounce. Keep the assertions; only the wait.
    maxWorkers: 2,
    testTimeout: 20_000,
    hookTimeout: 20_000,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"]
  }
});
