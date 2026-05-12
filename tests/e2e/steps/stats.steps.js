import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { Then } = createBdd(test);

Then('I see the stats overview', async ({ page }) => {
  await page.goto('/stats');
  await expect(
    page.getByRole('heading', { name: 'Statistics', level: 1 }),
  ).toBeVisible();
  await expect(page.locator('.stats-grid')).toBeVisible();
});

Then(
  'I see exercise-specific stats for the exercise I created',
  async ({ page, scenarioState }) => {
    await page.goto(`/stats/exercise/${scenarioState.exerciseId}`);
    await expect(
      page.getByRole('heading', {
        name: scenarioState.exerciseName,
        level: 1,
      }),
    ).toBeVisible();
    // Once any set has been logged, the chart SVG renders; the "No
    // progress data yet" fallback only appears for empty exercises.
    await expect(page.locator('#exercise-chart')).toBeVisible();
  },
);

Then('the PR list shows my exercise', async ({ page, scenarioState }) => {
  await page.goto('/stats/prs');
  await expect(
    page.getByRole('link', { name: scenarioState.exerciseName }),
  ).toBeVisible();
});
