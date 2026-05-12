import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { Given, When, Then } = createBdd(test);

async function shareCurrentWorkout(page, scenarioState) {
  await page.goto(`/workouts/${scenarioState.workoutId}`);
  await page
    .locator('form[action$="/share"]')
    .getByRole('button', { name: 'Share' })
    .click();
  const link = page.locator('.share-info a');
  await expect(link).toBeVisible();
  scenarioState.shareUrl = await link.getAttribute('href');
}

When('I share the workout', async ({ page, scenarioState }) => {
  await shareCurrentWorkout(page, scenarioState);
});

Given('I have shared the workout', async ({ page, scenarioState }) => {
  await shareCurrentWorkout(page, scenarioState);
});

When('I revoke the share', async ({ page, scenarioState }) => {
  await page.goto(`/workouts/${scenarioState.workoutId}`);
  page.once('dialog', (d) => d.accept());
  await page.getByRole('button', { name: 'Revoke Share' }).click();
  await expect(page.locator('.share-info')).toHaveCount(0);
});

Then(
  'a public share link is shown on the workout page',
  async ({ page, scenarioState }) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    const link = page.locator('.share-info a');
    await expect(link).toBeVisible();
    expect(scenarioState.shareUrl).toMatch(/^\/shared\/[A-Za-z0-9_-]+$/);
  },
);

Then(
  'a guest can view the workout via the share URL',
  async ({ browser, scenarioState }) => {
    const guest = await browser.newContext();
    try {
      const guestPage = await guest.newPage();
      const response = await guestPage.goto(scenarioState.shareUrl);
      expect(response?.status()).toBe(200);
      await expect(
        guestPage.locator('.subtitle').first(),
      ).toContainText('Shared by');
      await expect(guestPage.locator('.set-row').first()).toBeVisible();
    } finally {
      await guest.close();
    }
  },
);

Then(
  'a guest visiting the share URL gets a 404',
  async ({ browser, scenarioState }) => {
    const guest = await browser.newContext();
    try {
      const guestPage = await guest.newPage();
      const response = await guestPage.goto(scenarioState.shareUrl);
      expect(response?.status()).toBe(404);
    } finally {
      await guest.close();
    }
  },
);
