import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { When, Then } = createBdd(test);

When('I visit {string}', async ({ page }, path) => {
  await page.goto(path);
});

Then('the URL is {string}', async ({ page }, path) => {
  await expect(page).toHaveURL(path);
});
