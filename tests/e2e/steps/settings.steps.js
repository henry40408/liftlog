import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

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
