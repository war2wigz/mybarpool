import { BaseSequencer, type TestSpecification } from "vitest/node";
import { defineConfig } from "vitest/config";

/**
 * Files run one at a time, in path order: `config.test.ts` initialises the
 * single config PDA and must see it absent, so it has to run before
 * `entropy.test.ts` and `games.test.ts`. Vitest's default order follows cached
 * durations and would let the suites race on the config (build plan Step 3).
 */
class PathOrderSequencer extends BaseSequencer {
  override async sort(files: TestSpecification[]): Promise<TestSpecification[]> {
    return [...files].sort((a, b) => a.moduleId.localeCompare(b.moduleId));
  }
}

// Localnet tests run against Surfpool started by `anchor test`; the fork
// fetches accounts from mainnet on first touch, so allow for a slow network.
export default defineConfig({
  test: {
    include: ["**/*.test.ts"],
    testTimeout: 60_000,
    hookTimeout: 60_000,
    fileParallelism: false,
    sequence: { sequencer: PathOrderSequencer },
  },
});
