import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: ["src/**/*.ts"],
      exclude: ["src/index.ts"],
      reporter: ["text", "text-summary"],
      // Build plan Step 1: 100% branch coverage on the fee and winner functions;
      // the package as a whole not below 95% lines.
      thresholds: {
        lines: 95,
        "src/fees.ts": { branches: 100, lines: 100 },
        "src/winner.ts": { branches: 100, lines: 100 },
      },
    },
  },
});
