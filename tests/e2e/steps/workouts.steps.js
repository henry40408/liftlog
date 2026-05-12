import { expect } from '@playwright/test';
import { createBdd } from 'playwright-bdd';
import { test } from './fixtures.js';

const { Given, When, Then } = createBdd(test);

function workoutIdFromUrl(page) {
  const match = new URL(page.url()).pathname.match(/^\/workouts\/([0-9a-f-]+)/i);
  if (!match) throw new Error(`not on a workout detail page: ${page.url()}`);
  return match[1];
}

async function createTodaysWorkout(page, scenarioState) {
  await page.goto('/workouts/new');
  await page.getByRole('button', { name: 'Create Workout' }).click();
  await expect(page).toHaveURL(/^.*\/workouts\/[0-9a-f-]+$/i);
  scenarioState.workoutId = workoutIdFromUrl(page);
}

async function logSet(page, exerciseName, weight, reps) {
  await page
    .locator('select#exercise_id')
    .selectOption({ label: exerciseName });
  await page.getByLabel('Weight').fill(String(weight));
  await page.getByLabel('Reps').fill(String(reps));
  await page.getByRole('button', { name: 'Add Set' }).click();
  // The form posts and re-renders /workouts/{id}; wait for the new row.
  await expect(
    page.locator('.set-row').filter({ hasText: exerciseName }).first(),
  ).toBeVisible();
}

When('I start a new workout for today', async ({ page, scenarioState }) => {
  await createTodaysWorkout(page, scenarioState);
});

Given('I have a workout', async ({ page, scenarioState }) => {
  await createTodaysWorkout(page, scenarioState);
});

When(
  'I log a set of {int} kg for {int} reps using the exercise I created',
  async ({ page, scenarioState }, weight, reps) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    await logSet(page, scenarioState.exerciseName, weight, reps);
  },
);

Given(
  'I have a workout with a set of {int} kg for {int} reps',
  async ({ page, scenarioState }, weight, reps) => {
    if (!scenarioState.exerciseName) {
      throw new Error('this step requires an exercise to exist first');
    }
    await createTodaysWorkout(page, scenarioState);
    await logSet(page, scenarioState.exerciseName, weight, reps);
  },
);

When(
  'I edit my set to {int} kg for {int} reps',
  async ({ page, scenarioState }, weight, reps) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    const row = page
      .locator('.set-row')
      .filter({ hasText: scenarioState.exerciseName })
      .first();
    await row.getByRole('link', { name: 'Edit' }).click();
    await expect(
      page.getByRole('heading', { name: 'Edit Set', level: 1 }),
    ).toBeVisible();
    await page.getByLabel('Weight').fill(String(weight));
    await page.getByLabel('Reps').fill(String(reps));
    await page.getByRole('button', { name: 'Save Changes' }).click();
    await expect(page).toHaveURL(`/workouts/${scenarioState.workoutId}`);
  },
);

When('I delete the workout', async ({ page, scenarioState }) => {
  await page.goto(`/workouts/${scenarioState.workoutId}`);
  page.once('dialog', (d) => d.accept());
  await page
    .locator('form[action$="/delete"]')
    .filter({ has: page.getByRole('button', { name: 'Delete' }) })
    .first()
    .getByRole('button', { name: 'Delete' })
    .click();
  await expect(page).toHaveURL('/workouts');
});

Then('I am on the workout detail page', async ({ page, scenarioState }) => {
  expect(workoutIdFromUrl(page)).toBe(scenarioState.workoutId);
});

Then(
  'the workout I created is listed on the workouts page',
  async ({ page, scenarioState }) => {
    await page.goto('/workouts');
    await expect(
      page.locator(`a[href="/workouts/${scenarioState.workoutId}"]`),
    ).toBeVisible();
  },
);

Then(
  'the workout I deleted is not listed on the workouts page',
  async ({ page, scenarioState }) => {
    await page.goto('/workouts');
    await expect(
      page.locator(`a[href="/workouts/${scenarioState.workoutId}"]`),
    ).toHaveCount(0);
  },
);

Then(
  'I see my set logged at {int} kg for {int} reps',
  async ({ page, scenarioState }, weight, reps) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    const row = page
      .locator('.set-row')
      .filter({ hasText: scenarioState.exerciseName })
      .first();
    await expect(row.locator('.set-cell-weight')).toHaveText(String(weight));
    await expect(row.locator('.set-cell-reps')).toHaveText(String(reps));
  },
);
