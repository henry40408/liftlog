import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { Then } = createBdd(test);

Then(
  'the dashboard lists the workout I created in Recent Workouts',
  async ({ page, scenarioState }) => {
    await page.goto('/');
    await expect(
      page.getByRole('heading', { name: 'Recent Workouts', level: 2 }),
    ).toBeVisible();
    await expect(
      page.locator(`.workout-list a[href="/workouts/${scenarioState.workoutId}"]`),
    ).toBeVisible();
  },
);

Then(
  'the dashboard {string} count is {int}',
  async ({ page }, label, count) => {
    await page.goto('/');
    const card = page.locator('.stat-card').filter({ hasText: label });
    await expect(card.locator('.stat-value')).toHaveText(String(count));
  },
);
