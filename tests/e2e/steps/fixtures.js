import { test as base } from 'playwright-bdd';
import { randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync, rmSync } from 'node:fs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, '../../..');
const BASE_PORT = Number(process.env.E2E_BASE_PORT ?? 3100);
const HEALTH_TIMEOUT_MS = 60_000;

async function waitForHealth(baseURL) {
  const deadline = Date.now() + HEALTH_TIMEOUT_MS;
  let lastErr;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${baseURL}/health`);
      if (res.ok) return;
    } catch (err) {
      lastErr = err;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(
    `server at ${baseURL} did not come up within ${HEALTH_TIMEOUT_MS}ms: ${lastErr?.message ?? 'timeout'}`,
  );
}

// One Rust server per Playwright worker, isolated on its own port and
// sqlite file. The binary is expected to already be built (the npm test
// script runs `cargo build` first); we just spawn it. Each worker also
// gets a fresh DB so _bootstrap.feature can rely on "no users yet."
export const test = base.extend({
  workerServer: [
    async ({}, use, workerInfo) => {
      const port = BASE_PORT + workerInfo.workerIndex;
      const dbRel = `tests/e2e/.tmp/liftlog-e2e-${workerInfo.workerIndex}.sqlite3`;
      const dbAbs = resolve(REPO_ROOT, dbRel);
      mkdirSync(dirname(dbAbs), { recursive: true });
      try {
        rmSync(dbAbs);
      } catch {
        // first run; nothing to clean
      }

      const baseURL = `http://127.0.0.1:${port}`;
      const child = spawn(resolve(REPO_ROOT, 'target/debug/liftlog'), [], {
        cwd: REPO_ROOT,
        env: {
          ...process.env,
          DATABASE_URL: `sqlite:${dbRel}?mode=rwc`,
          HOST: '127.0.0.1',
          PORT: String(port),
          RUST_LOG: 'liftlog=warn',
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      });

      // Drain stdio so a server that logs a lot doesn't block on a full
      // pipe buffer mid-test.
      child.stdout?.on('data', () => {});
      child.stderr?.on('data', () => {});

      try {
        await waitForHealth(baseURL);
        await use({ baseURL });
      } finally {
        child.kill('SIGTERM');
        await new Promise((r) => {
          if (child.exitCode !== null) return r();
          child.once('exit', () => r());
        });
      }
    },
    { scope: 'worker' },
  ],

  // Point Playwright's built-in baseURL at this worker's server so
  // page.goto('/path') and request.post('/path') resolve to the right
  // port without any test-side knowledge of which worker we're on.
  // baseURL is test-scoped in Playwright, so we keep it test-scoped here.
  baseURL: async ({ workerServer }, use) => {
    await use(workerServer.baseURL);
  },

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
