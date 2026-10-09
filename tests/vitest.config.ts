import { sep } from "node:path";

import { BaseSequencer, type TestSpecification } from "vitest/node";
import { defineConfig } from "vitest/config";

/**
 * Files run one at a time, in path order: `config.test.ts` initialises the
 * single config PDA and must see it absent, so it has to run before
 * `entropy.test.ts` and `games.test.ts`. Vitest's default order follows cached
 * durations and would let the suites race on the config (build plan Step 3).
 *
 * Every file under `lifecycle/` sorts after every file outside it (path order
 * within): `unresolved.test.ts` jumps the clock thirty days once, and
 * `lifecycle/08-abandoned.test.ts` jumps it again and nothing may follow
 * (build plan Step 8).
 */
const LIFECYCLE = `${sep}lifecycle${sep}`;

class PathOrderSequencer extends BaseSequencer {
  override async sort(files: TestSpecification[]): Promise<TestSpecification[]> {
    const rank = (id: string) => (id.includes(LIFECYCLE) ? 1 : 0);
    return [...files].sort(
      (a, b) => rank(a.moduleId) - rank(b.moduleId) || a.moduleId.localeCompare(b.moduleId),
    );
  }
}

// Localnet tests run against Surfpool started by `anchor test`; the fork
// fetches accounts from mainnet on first touch, so allow for a slow network.
export default defineConfig({
  test: {
    include: ["**/*.test.ts"],
    // The lifecycle scripts set their own 120 s per phase (a draw waits for the Entropy sample
    // window) and a longer `beforeAll` for the fill and draw; see each file.
    testTimeout: 60_000,
    hookTimeout: 60_000,
    fileParallelism: false,
    sequence: { sequencer: PathOrderSequencer },
    // The suites report measured compute units, time-travel targets and transaction profiles
    // through console.log; vitest 5 hides the output of passing tests unless told otherwise,
    // and the numbers are what NOTES.md and the CI log record (Step 7).
    silent: false,
  },
});
