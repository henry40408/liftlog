import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { When, Then } = createBdd(test);

When(
  'I change my password from {string} to {string}',
  async ({ page }, currentPassword, newPassword) => {
    await page.goto('/settings');
    await page.getByLabel('Current Password').fill(currentPassword);
    await page.getByLabel('New Password', { exact: true }).fill(newPassword);
    await page.getByLabel('Confirm New Password').fill(newPassword);
    await page.getByRole('button', { name: 'Change Password' }).click();
  },
);

Then('I see a password-change success message', async ({ page }) => {
  await expect(page.locator('.alert-success')).toContainText(
    'Password changed successfully',
  );
});
