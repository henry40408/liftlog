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

When('I promote that user to admin', async ({ page, scenarioState }) => {
  await page.goto('/users');
  page.once('dialog', (d) => d.accept());
  await userRow(page, scenarioState.otherUser)
    .getByRole('button', { name: 'Promote' })
    .click();
  await expect(page).toHaveURL('/users');
});

When('I delete that user', async ({ page, scenarioState }) => {
  await page.goto('/users');
  page.once('dialog', (d) => d.accept());
  await userRow(page, scenarioState.otherUser)
    .getByRole('button', { name: 'Delete' })
    .click();
  await expect(page).toHaveURL('/users');
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
