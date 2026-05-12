import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { Given, When, Then } = createBdd(test);

async function createExercise(page, scenarioState, name, category) {
  await page.goto('/exercises/new');
  await page.getByLabel('Name').fill(name);
  await page.getByLabel('Category').selectOption(category);
  await page.getByRole('button', { name: 'Add Exercise' }).click();
  await expect(page).toHaveURL('/exercises');
  scenarioState.exerciseName = name;
  // /exercises lists each exercise as <a href="/stats/exercise/{id}">{name}</a>.
  const href = await page
    .getByRole('link', { name })
    .first()
    .getAttribute('href');
  scenarioState.exerciseId =
    href?.match(/\/stats\/exercise\/([^/]+)/)?.[1] ?? null;
}

When(
  'I create a new exercise in category {string}',
  async ({ page, scenarioState }, category) => {
    await createExercise(page, scenarioState, scenarioState.unique('Squat'), category);
  },
);

Given(
  'I have an exercise in category {string}',
  async ({ page, scenarioState }, category) => {
    await createExercise(page, scenarioState, scenarioState.unique('Exercise'), category);
  },
);

Then(
  'the exercise I created is listed on the exercises page',
  async ({ page, scenarioState }) => {
    await page.goto('/exercises');
    await expect(
      page.getByRole('link', { name: scenarioState.exerciseName }),
    ).toBeVisible();
  },
);

When(
  'I rename my exercise',
  async ({ page, scenarioState }) => {
    await page.goto('/exercises');
    const row = page
      .locator('.exercise-item')
      .filter({
        has: page.getByRole('link', { name: scenarioState.exerciseName }),
      })
      .first();
    await row.getByRole('link', { name: 'Edit' }).click();
    const newName = scenarioState.unique('Renamed');
    await page.getByLabel('Name').fill(newName);
    await page.getByRole('button', { name: 'Save Changes' }).click();
    await expect(page).toHaveURL('/exercises');
    scenarioState.exerciseName = newName;
  },
);

When('I delete my exercise', async ({ page, scenarioState }) => {
  await page.goto('/exercises');
  const row = page
    .locator('.exercise-item')
    .filter({
      has: page.getByRole('link', { name: scenarioState.exerciseName }),
    })
    .first();
  page.once('dialog', (d) => d.accept());
  await row.getByRole('button', { name: 'Delete' }).click();
});

Then(
  'my exercise is no longer listed on the exercises page',
  async ({ page, scenarioState }) => {
    await page.goto('/exercises');
    await expect(
      page.getByRole('link', { name: scenarioState.exerciseName }),
    ).toHaveCount(0);
  },
);
