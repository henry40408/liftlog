import { expect } from '@playwright/test';

// Every scenario's fixtures hang off this account, and it is created through
// the real /auth/setup endpoint, so its password has to satisfy the server's
// policy (length floor plus a zxcvbn score >= 3 — see
// models::user::password_policy_error). Deliberately comfortably above that
// bar rather than exactly on it: the previous value scored exactly 3, so
// raising the threshold would have failed the whole suite at the seeding
// step, where the cause is least visible.
export const ADMIN = { username: 'lifter', password: 'barbell-club-2026' };

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
