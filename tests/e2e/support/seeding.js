import { expect } from '@playwright/test';

export const ADMIN = { username: 'lifter', password: 'barbell-club' };

const STATUS_OK_OR_REDIRECT = [200, 302, 303];

// /auth/setup creates the first user (admin). It's idempotent for our needs:
// re-running 302s to /auth/login because user_count > 0.
async function callSetup(request, baseURL, { username, password }) {
  const res = await request.post(`${baseURL}/auth/setup`, {
    form: { username, password },
    maxRedirects: 0,
    failOnStatusCode: false,
  });
  expect(
    STATUS_OK_OR_REDIRECT,
    `/auth/setup unexpected status ${res.status()}`,
  ).toContain(res.status());
}

async function adminLogin(request, baseURL) {
  const res = await request.post(`${baseURL}/auth/login`, {
    form: ADMIN,
    maxRedirects: 0,
    failOnStatusCode: false,
  });
  expect(
    [302, 303],
    `admin login expected redirect, got ${res.status()}`,
  ).toContain(res.status());
}

// Make sure the named user exists. The first user (admin) is created via
// /auth/setup; everyone else is created by the admin through /users/new.
// Idempotent: if a user with the same name already exists the create endpoint
// renders the same page with an error, which we ignore.
export async function ensureUser(request, baseURL, username, password) {
  if (username === ADMIN.username) {
    await callSetup(request, baseURL, ADMIN);
    return;
  }
  await callSetup(request, baseURL, ADMIN);
  await adminLogin(request, baseURL);
  await request.post(`${baseURL}/users/new`, {
    form: { username, password },
    maxRedirects: 0,
    failOnStatusCode: false,
  });
}
