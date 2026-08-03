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

Then(
  'the PR list shows {int} for my exercise in both the all-time and 1-month columns',
  async ({ page, scenarioState }, weight) => {
    await page.goto('/stats/prs');
    const row = page.locator('tbody tr').filter({
      has: page.getByRole('link', { name: scenarioState.exerciseName }),
    });
    await expect(row.locator('td[data-label="PR (All)"]')).toHaveText(
      String(weight),
    );
    // Just logged, so the rolling 1-month window carries the same number.
    await expect(row.locator('td[data-label="PR (1M)"]')).toHaveText(
      String(weight),
    );
  },
);
