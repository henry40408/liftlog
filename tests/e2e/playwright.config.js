import { defineConfig, devices } from '@playwright/test';
import { defineBddConfig } from 'playwright-bdd';

const testDir = defineBddConfig({
  features: 'features/**/*.feature',
  steps: 'steps/**/*.js',
});

// Each Playwright worker spawns its own Rust server + sqlite DB (see
// steps/fixtures.js > workerServer). We're free to go parallel.
const workers = process.env.WORKERS
  ? Number(process.env.WORKERS)
  : process.env.CI
    ? 2
    : '50%';

export default defineConfig({
  testDir,
  retries: 0,
  fullyParallel: true,
  workers,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    trace: 'retain-on-failure',
    video: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
