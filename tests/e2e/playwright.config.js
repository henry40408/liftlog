import { defineConfig, devices } from '@playwright/test';
import { defineBddConfig } from 'playwright-bdd';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '../..');

const testDir = defineBddConfig({
  features: 'features/**/*.feature',
  steps: 'steps/**/*.js',
});

const PORT = Number(process.env.E2E_PORT ?? 3100);
const BASE_URL = `http://127.0.0.1:${PORT}`;
const DB_PATH = 'tests/e2e/.tmp/liftlog-e2e.sqlite3';

export default defineConfig({
  testDir,
  retries: 0,
  // Single sqlite + a single first-time setup endpoint forces serial execution.
  fullyParallel: false,
  workers: 1,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: BASE_URL,
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
  webServer: {
    command: `mkdir -p tests/e2e/.tmp && rm -f ${DB_PATH} && cargo run --quiet`,
    cwd: repoRoot,
    url: `${BASE_URL}/health`,
    reuseExistingServer: false,
    timeout: 240_000,
    stdout: 'pipe',
    stderr: 'pipe',
    env: {
      DATABASE_URL: `sqlite:${DB_PATH}?mode=rwc`,
      HOST: '127.0.0.1',
      PORT: String(PORT),
      RUST_LOG: 'liftlog=warn',
    },
  },
});
