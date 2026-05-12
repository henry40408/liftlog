import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';
import { ADMIN } from '../support/seeding.js';

const { When, Then } = createBdd(test);

async function fillPasswordForm(page, current, next, confirm) {
  await page.goto('/settings');
  await page.getByLabel('Current Password').fill(current);
  await page.getByLabel('New Password', { exact: true }).fill(next);
  await page.getByLabel('Confirm New Password').fill(confirm);
  await page.getByRole('button', { name: 'Change Password' }).click();
}

When(
  'I change my password from {string} to {string}',
  async ({ page }, current, next) => {
    await fillPasswordForm(page, current, next, next);
  },
);

When(
  'I submit the password form with current {string}, new {string}, confirm {string}',
  async ({ page }, current, next, confirm) => {
    await fillPasswordForm(page, current, next, confirm);
  },
);

Then('I see a password-change success message', async ({ page }) => {
  await expect(page.locator('.alert-success')).toContainText(
    'Password changed successfully',
  );
});

Then('I see a settings error {string}', async ({ page }, message) => {
  await expect(page.locator('.error')).toContainText(message);
});

function sessionsTable(page) {
  return page
    .locator('table.data-table')
    .filter({
      has: page.getByRole('columnheader', { name: 'Device' }),
    });
}

When(
  'I have a second session as {string}',
  async ({ playwright, baseURL }, username) => {
    const ctx = await playwright.request.newContext({ baseURL });
    try {
      const res = await ctx.post('/auth/login', {
        form: { username, password: ADMIN.password },
        maxRedirects: 0,
        failOnStatusCode: false,
      });
      expect(
        [302, 303],
        `second-session login expected redirect, got ${res.status()}`,
      ).toContain(res.status());
    } finally {
      await ctx.dispose();
    }
  },
);

When('I log out all other devices', async ({ page }) => {
  await page.goto('/settings');
  page.once('dialog', (d) => d.accept());
  await page
    .getByRole('button', { name: 'Log out all other devices' })
    .click();
});

Then(
  'the active sessions table has {int} row(s)',
  async ({ page }, count) => {
    await page.goto('/settings');
    await expect(sessionsTable(page).locator('tbody tr')).toHaveCount(count);
  },
);

Then('the active sessions table marks my current device', async ({ page }) => {
  await page.goto('/settings');
  await expect(sessionsTable(page).getByText('This device')).toBeVisible();
});
