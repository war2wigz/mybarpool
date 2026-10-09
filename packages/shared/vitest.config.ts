import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: ["src/**/*.ts"],
      // Generated code is not ours to cover; the hand-written layer that calls it is.
      exclude: ["src/index.ts", "src/generated/**"],
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
