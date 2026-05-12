import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';
import { ADMIN, ensureUser } from '../support/seeding.js';

const { Given, When, Then } = createBdd(test);

async function loginViaUi(page, username, password) {
  await page.goto('/auth/login');
  await page.getByLabel('Username').fill(username);
  await page.getByLabel('Password').fill(password);
  await page.getByRole('button', { name: 'Login' }).click();
}

Given(
  'a user {string} with password {string} exists',
  async ({ request, baseURL }, username, password) => {
    await ensureUser(request, baseURL, username, password);
  },
);

Given('I am logged in as {string}', async ({ page, request, baseURL }, username) => {
  await ensureUser(request, baseURL, username, ADMIN.password);
  await loginViaUi(page, username, ADMIN.password);
  await expect(
    page.getByRole('heading', { name: 'Dashboard', level: 1 }),
  ).toBeVisible();
});

Given(
  'I am logged in as {string} with password {string}',
  async ({ page, request, baseURL }, username, password) => {
    await ensureUser(request, baseURL, username, password);
    await loginViaUi(page, username, password);
    await expect(
      page.getByRole('heading', { name: 'Dashboard', level: 1 }),
    ).toBeVisible();
  },
);

When(
  'I log in as {string} with password {string}',
  async ({ page }, username, password) => {
    await loginViaUi(page, username, password);
  },
);

When('I log out', async ({ page }) => {
  await page.getByRole('button', { name: 'Sign Out' }).click();
});

Then('I see the dashboard', async ({ page }) => {
  await expect(page).toHaveURL('/');
  await expect(
    page.getByRole('heading', { name: 'Dashboard', level: 1 }),
  ).toBeVisible();
});

Then('I see the login page', async ({ page }) => {
  await expect(page).toHaveURL('/auth/login');
  await expect(
    page.getByRole('button', { name: 'Login' }),
  ).toBeVisible();
});

Then('I see the setup page', async ({ page }) => {
  await expect(page).toHaveURL('/auth/setup');
  await expect(
    page.getByRole('button', { name: 'Create Account' }),
  ).toBeVisible();
});

Then('I see the login error {string}', async ({ page }, message) => {
  await expect(page.locator('.error')).toContainText(message);
});

When(
  'I submit the setup form with username {string} and password {string}',
  async ({ page }, username, password) => {
    await page.goto('/auth/setup');
    // Bypass the browser's own minlength/required validation so the
    // request actually hits the server. We want to lock the server-side
    // defense-in-depth check, not the HTML attributes.
    await page.locator('form').evaluate((f) => {
      f.noValidate = true;
    });
    await page.getByLabel('Username').fill(username);
    await page.locator('#password').fill(password);
    await page.getByRole('button', { name: 'Create Account' }).click();
  },
);

Then('I see the setup error {string}', async ({ page }, message) => {
  await expect(page).toHaveURL('/auth/setup');
  await expect(page.locator('.error')).toContainText(message);
});

Given(
  'I am logged in as a fresh non-admin user',
  async ({ page, request, baseURL, scenarioState }) => {
    const username = scenarioState.unique('member');
    scenarioState.currentUser = username;
    await ensureUser(request, baseURL, username, ADMIN.password);
    await loginViaUi(page, username, ADMIN.password);
    await expect(
      page.getByRole('heading', { name: 'Dashboard', level: 1 }),
    ).toBeVisible();
  },
);
