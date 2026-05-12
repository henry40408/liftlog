import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { Given, When, Then } = createBdd(test);

// The Rust server boots with an empty sqlite DB per run, so /auth/setup is
// the only path that can create the first user. Subsequent calls 302 back to
// /auth/login, which is treated as a no-op here.
Given(
  'a user {string} with password {string} exists',
  async ({ request, baseURL }, username, password) => {
    const res = await request.post(`${baseURL}/auth/setup`, {
      form: { username, password },
      maxRedirects: 0,
      failOnStatusCode: false,
    });
    expect(
      [200, 302, 303],
      `unexpected /auth/setup status ${res.status()}`,
    ).toContain(res.status());
  },
);

When(
  'I log in as {string} with password {string}',
  async ({ page }, username, password) => {
    await page.goto('/auth/login');
    await page.getByLabel('Username').fill(username);
    await page.getByLabel('Password').fill(password);
    await page.getByRole('button', { name: 'Login' }).click();
  },
);

Then('I see the dashboard', async ({ page }) => {
  await expect(page).toHaveURL('/');
  await expect(
    page.getByRole('heading', { name: 'Dashboard', level: 1 }),
  ).toBeVisible();
});
