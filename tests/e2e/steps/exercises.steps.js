import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { Given, When, Then } = createBdd(test);

async function createExercise(page, name, category) {
  await page.goto('/exercises/new');
  await page.getByLabel('Name').fill(name);
  await page.getByLabel('Category').selectOption(category);
  await page.getByRole('button', { name: 'Add Exercise' }).click();
  await expect(page).toHaveURL('/exercises');
}

When(
  'I create a new exercise in category {string}',
  async ({ page, scenarioState }, category) => {
    const name = scenarioState.unique('Squat');
    await createExercise(page, name, category);
    scenarioState.exerciseName = name;
  },
);

Given(
  'I have an exercise in category {string}',
  async ({ page, scenarioState }, category) => {
    const name = scenarioState.unique('Exercise');
    await createExercise(page, name, category);
    scenarioState.exerciseName = name;
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
