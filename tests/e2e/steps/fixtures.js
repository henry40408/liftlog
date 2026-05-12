import { test as base } from 'playwright-bdd';
import { randomBytes } from 'node:crypto';

// Each scenario gets a freshly-cleared browser context plus a scratch state
// object scoped to that scenario. We share one sqlite DB across the whole
// run, so steps must use `state.suffix` to scope created entities and assert
// only on what this scenario built.
export const test = base.extend({
  context: async ({ context }, use) => {
    await context.clearCookies();
    await use(context);
  },
  scenarioState: async ({}, use) => {
    const suffix = randomBytes(4).toString('hex');
    await use({
      suffix,
      unique: (prefix) => `${prefix}-${suffix}`,
      workoutId: null,
      exerciseId: null,
      exerciseName: null,
      shareUrl: null,
    });
  },
});
