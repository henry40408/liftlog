import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';
import { ADMIN, ensureUser } from '../support/seeding.js';

const { When, Then } = createBdd(test);

async function fillPasswordForm(page, current, next, confirm) {
  await page.goto('/settings');
  // Same reason as the setup form's short-password step in auth.steps.js:
  // the new-password input carries minlength/maxlength, which would block a
  // deliberately-invalid submission before it ever reaches the server. The
  // server-side length check is the actual control, so bypass the browser's
  // validation to keep it under test.
  await page.locator('form[action="/settings/password"]').evaluate((f) => {
    f.noValidate = true;
  });
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

// The trigger is a link to a confirmation page now. The POST renders the
// settings page in place rather than redirecting, so the URL stays at
// /settings/logout-others — assert on the success alert instead.
When('I log out all other devices', async ({ page }) => {
  await page.goto('/settings');
  await page.getByRole('link', { name: 'Log out all other devices' }).click();
  await page.getByRole('button', { name: 'Log out other devices' }).click();
  await expect(page.locator('.alert-success')).toContainText(
    'Logged out of all other devices.',
  );
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

// Under 480px the data-table collapses to cards and the column headers are
// hidden; td::before re-prints them from data-label. Without it the two
// timestamps sit next to each other with nothing telling them apart.
Then('the active sessions table labels every cell', async ({ page }) => {
  await page.goto('/settings');
  const labels = await sessionsTable(page)
    .locator('tbody tr')
    .first()
    .locator('td')
    .evaluateAll((tds) => tds.map((td) => td.dataset.label));
  expect(labels).toEqual(['Device', 'Last active', 'Signed in']);
});

Then(
  'the settings page in timezone {string} shows session times ending with {string} for {string} with password {string}',
  async (
    { browser, request, baseURL },
    timezoneId,
    tzSuffix,
    username,
    password,
  ) => {
    await ensureUser(request, baseURL, username, password);
    // A fresh context is the only way to pin the timezone — timezoneId is
    // fixed at context creation, and the shared `page` fixture is already
    // running on the machine's own.
    const ctx = await browser.newContext({ baseURL, timezoneId });
    try {
      const tzPage = await ctx.newPage();
      await tzPage.goto('/auth/login');
      await tzPage.getByLabel('Username').fill(username);
      await tzPage.getByLabel('Password').fill(password);
      await tzPage.getByRole('button', { name: 'Login' }).click();
      await tzPage.goto('/settings');

      const times = sessionsTable(tzPage)
        .locator('tbody tr')
        .first()
        .locator('time');
      await expect(times).toHaveCount(2);
      const expected = new RegExp(
        `^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2} ${tzSuffix.replace('+', '\\+')}$`,
      );
      for (const text of await times.allTextContents()) {
        expect(text).toMatch(expected);
      }
    } finally {
      await ctx.close();
    }
  },
);
