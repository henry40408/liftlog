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

When('I delete my set', async ({ page, scenarioState }) => {
  await page.goto(`/workouts/${scenarioState.workoutId}`);
  const row = page
    .locator('.set-row')
    .filter({ hasText: scenarioState.exerciseName })
    .first();
  page.once('dialog', (d) => d.accept());
  await row.locator('form[action*="/logs/"][action$="/delete"] button').click();
});

Then(
  'my set is no longer shown on the workout',
  async ({ page, scenarioState }) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    await expect(
      page.locator('.set-row').filter({ hasText: scenarioState.exerciseName }),
    ).toHaveCount(0);
  },
);

When(
  'I edit the workout to date {string} with notes {string}',
  async ({ page, scenarioState }, date, notes) => {
    await page.goto(`/workouts/${scenarioState.workoutId}/edit`);
    await page.getByLabel('Date').fill(date);
    await page.getByLabel('Notes (optional)').fill(notes);
    await page.getByRole('button', { name: 'Save Changes' }).click();
    await expect(page).toHaveURL(`/workouts/${scenarioState.workoutId}`);
  },
);

Then(
  'the workout detail shows date {string} and notes {string}',
  async ({ page, scenarioState }, date, notes) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    await expect(
      page.getByRole('heading', { level: 1, name: date }),
    ).toBeVisible();
    await expect(page.locator('.subtitle em')).toHaveText(notes);
  },
);

Then('my set is flagged as a PR', async ({ page, scenarioState }) => {
  await page.goto(`/workouts/${scenarioState.workoutId}`);
  const row = page
    .locator('.set-row')
    .filter({ hasText: scenarioState.exerciseName })
    .first();
  await expect(row.locator('.pr-badge')).toBeVisible();
});

When(
  'I log a set of {int} kg for {int} reps with RPE {int} using the exercise I created',
  async ({ page, scenarioState }, weight, reps, rpe) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    await page
      .locator('select#exercise_id')
      .selectOption({ label: scenarioState.exerciseName });
    await page.getByLabel('Weight').fill(String(weight));
    await page.getByLabel('Reps').fill(String(reps));
    await page.getByLabel('RPE (1-10, optional)').fill(String(rpe));
    await page.getByRole('button', { name: 'Add Set' }).click();
    await expect(
      page.locator('.set-row').filter({ hasText: scenarioState.exerciseName }).first(),
    ).toBeVisible();
  },
);

Then(
  'I see my set logged with RPE {int}',
  async ({ page, scenarioState }, rpe) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    const row = page
      .locator('.set-row')
      .filter({ hasText: scenarioState.exerciseName })
      .first();
    await expect(row.locator('.set-cell-rpe')).toHaveText(String(rpe));
  },
);

When(
  'I log another set of {int} kg for {int} reps using the same exercise',
  async ({ page, scenarioState }, weight, reps) => {
    await page.goto(`/workouts/${scenarioState.workoutId}`);
    await page
      .locator('select#exercise_id')
      .selectOption({ label: scenarioState.exerciseName });
    await page.getByLabel('Weight').fill(String(weight));
    await page.getByLabel('Reps').fill(String(reps));
    await page.getByRole('button', { name: 'Add Set' }).click();
  },
);

Then('I see two sets numbered 1 and 2', async ({ page, scenarioState }) => {
  await page.goto(`/workouts/${scenarioState.workoutId}`);
  const rows = page
    .locator('.set-row')
    .filter({ hasText: scenarioState.exerciseName });
  await expect(rows).toHaveCount(2);
  const numbers = (
    await rows.locator('.set-cell-set').allTextContents()
  ).sort();
  expect(numbers).toEqual(['1', '2']);
});

When('I click clone on my set', async ({ page, scenarioState }) => {
  await page.goto(`/workouts/${scenarioState.workoutId}`);
  const row = page
    .locator('.set-row')
    .filter({ hasText: scenarioState.exerciseName })
    .first();
  await row.getByRole('button', { name: 'Clone' }).click();
});

Then(
  'the Add Set form is pre-filled with weight {int} and reps {int}',
  async ({ page, scenarioState }, weight, reps) => {
    expect(await page.locator('select#exercise_id').inputValue()).toBe(
      scenarioState.exerciseId,
    );
    await expect(page.getByLabel('Weight')).toHaveValue(String(weight));
    await expect(page.getByLabel('Reps')).toHaveValue(String(reps));
  },
);

Then(
  'visiting the workout I created returns a 404',
  async ({ page, scenarioState }) => {
    const response = await page.goto(`/workouts/${scenarioState.workoutId}`);
    expect(response?.status()).toBe(404);
  },
);

Then(
  'I do not see the workout I created on the workouts page',
  async ({ page, scenarioState }) => {
    await page.goto('/workouts');
    await expect(
      page.locator(`a[href="/workouts/${scenarioState.workoutId}"]`),
    ).toHaveCount(0);
  },
);

Then('I see the workouts empty state', async ({ page }) => {
  await page.goto('/workouts');
  await expect(page.locator('.empty-state')).toContainText('No workouts yet');
});

Then('visiting {string} returns a 404', async ({ page }, path) => {
  const response = await page.goto(path);
  expect(response?.status()).toBe(404);
});
