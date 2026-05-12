import { test as base } from 'playwright-bdd';

// Every scenario starts logged-out. Cookies wouldn't normally persist across
// scenarios anyway (a fresh browser context is created), but being explicit
// keeps the contract obvious.
export const test = base.extend({
  context: async ({ context }, use) => {
    await context.clearCookies();
    await use(context);
  },
});
