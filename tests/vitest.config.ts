import { defineConfig } from "vitest/config";

// Localnet tests run against Surfpool started by `anchor test`; the fork
// fetches accounts from mainnet on first touch, so allow for a slow network.
export default defineConfig({
  test: {
    include: ["**/*.test.ts"],
    testTimeout: 60_000,
    hookTimeout: 60_000,
  },
});
