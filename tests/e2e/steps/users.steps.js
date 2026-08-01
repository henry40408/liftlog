import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';
import { ADMIN, ensureUser } from '../support/seeding.js';

const { Given, When, Then } = createBdd(test);

function userRow(page, username) {
  return page.locator('tr').filter({ hasText: username }).first();
}

Given(
  'another user exists',
  async ({ request, baseURL, scenarioState }) => {
    const username = scenarioState.unique('subject');
    scenarioState.otherUser = username;
    await ensureUser(request, baseURL, username, ADMIN.password);
  },
);

When(
  'I create a new user via the admin UI',
  async ({ page, scenarioState }) => {
    const username = scenarioState.unique('newbie');
    scenarioState.otherUser = username;
    await page.goto('/users/new');
    await page.getByLabel('Username').fill(username);
    await page.locator('#password').fill('starting-pass');
    await page.getByRole('button', { name: 'Create User' }).click();
    await expect(page).toHaveURL('/users');
  },
);

// Promote and delete no longer fire a window.confirm(); each opens a
// confirmation page that re-checks the admin's own password before acting,
// so the dialog handlers these steps used to install are gone with it.
//
// `linkName` is the row action on /users; `buttonName` is the confirmation
// page's submit button, worded in full ("Delete user", not "Delete"). Both
// are spelled out rather than leaning on getByRole's substring matching, so
// a reworded button fails loudly instead of quietly matching something else.
async function confirmUserAction(page, username, linkName, buttonName) {
  await page.goto('/users');
  await userRow(page, username).getByRole('link', { name: linkName }).click();
  await page
    .getByLabel('Confirm your password to continue')
    .fill(ADMIN.password);
  await page.getByRole('button', { name: buttonName, exact: true }).click();
  await expect(page).toHaveURL('/users');
}

When('I promote that user to admin', async ({ page, scenarioState }) => {
  await confirmUserAction(
    page,
    scenarioState.otherUser,
    'Promote',
    'Promote to admin',
  );
});

When('I delete that user', async ({ page, scenarioState }) => {
  await confirmUserAction(page, scenarioState.otherUser, 'Delete', 'Delete user');
});

When(
  'I open the delete confirmation for that user',
  async ({ page, scenarioState }) => {
    await page.goto('/users');
    await userRow(page, scenarioState.otherUser)
      .getByRole('link', { name: 'Delete' })
      .click();
  },
);

When('I confirm with the wrong password', async ({ page }) => {
  await page
    .getByLabel('Confirm your password to continue')
    .fill('definitely-not-it');
  await page.getByRole('button', { name: 'Delete user', exact: true }).click();
});

Then('I see a confirmation error', async ({ page }) => {
  await expect(page.locator('.error')).toContainText('Password is incorrect');
});

Then(
  'I see that user listed on the users page',
  async ({ page, scenarioState }) => {
    await page.goto('/users');
    await expect(userRow(page, scenarioState.otherUser)).toBeVisible();
  },
);

Then('I see that user listed as Admin', async ({ page, scenarioState }) => {
  await page.goto('/users');
  await expect(userRow(page, scenarioState.otherUser)).toContainText('admin');
});

Then(
  'I do not see that user on the users page',
  async ({ page, scenarioState }) => {
    await page.goto('/users');
    await expect(
      page.locator('tr').filter({ hasText: scenarioState.otherUser }),
    ).toHaveCount(0);
  },
);

Then(
  'I do not see the {string} button on the users page',
  async ({ page }, name) => {
    await page.goto('/users');
    await expect(
      page.getByRole('link', { name }),
    ).toHaveCount(0);
  },
);

Then('visiting {string} returns a 403', async ({ page }, path) => {
  const response = await page.goto(path);
  expect(response?.status()).toBe(403);
});

Then(
  'the users page does not let me delete my own account',
  async ({ page }) => {
    await page.goto('/users');
    const myRow = page.locator('tr').filter({ hasText: 'lifter' }).first();
    await expect(myRow.getByText('(you)')).toBeVisible();
    // A link now, not a submit button — the row action opens a confirmation
    // page rather than posting directly.
    await expect(myRow.getByRole('link', { name: 'Delete' })).toHaveCount(0);
  },
);
