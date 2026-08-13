# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

LiftLog is a self-hosted workout journal: an Axum + Askama server-rendered app backed by a single SQLite file, shipped as a Docker image.

## Commands

```bash
cargo run                                 # dev server on $LIFTLOG_BIND (default 127.0.0.1:8080)
cargo fmt --all -- --check                # formatting gate
cargo clippy --all-targets -- -D warnings # lint gate
cargo deny check                          # supply-chain gate (advisories/licenses/bans/sources)
cargo nextest run                         # Rust integration + unit tests
cargo nextest run --test workout_test     # single integration file
cargo nextest run -p liftlog session_repo # filter by name
```

UI BDD suite (Playwright + playwright-bdd, lives in `tests/e2e/`):

```bash
cd tests/e2e
npm install && npm run install-browsers   # one-time
npm test                                  # headless; boots cargo run on :3100
npm run test:ui                           # interactive runner
npx playwright test sharing               # filter by feature filename
```

## Architecture

**Single shared state.** `AppState` (`src/state.rs`) wires 4 repositories — `UserRepository`, `ExerciseRepository`, `WorkoutRepository`, `SessionRepository` — over an `r2d2` SQLite pool. Handlers take it via `State<AppState>`; there's no per-handler state.

**Sliding session middleware.** `sliding_session_middleware` runs globally on every route (`src/middleware/auth.rs`). It reads the session cookie, calls `SessionRepository::validate_and_touch`, and on success injects a `ValidatedSession` request extension carrying the full user identity. The `AuthUser` and `AdminUser` extractors read from that extension — they never hit the DB themselves. Routes that should never refresh the cookie (e.g. logout) insert `SuppressSessionRefresh`. Expiry is also swept periodically by a background tokio task spawned in `main.rs`.

**CSRF origin guard.** `csrf_origin_guard` (`src/middleware/csrf.rs`) is layered outermost — registered after the session layer so it runs *first* — and rejects any state-changing request a browser reports as cross-site (`Sec-Fetch-Site: cross-site`, or a mismatched `Origin` vs `Host`) with `403`. It is header-only; safe methods and header-less non-browser clients (curl, the test harness) pass through. Together with the session cookie's `SameSite=Lax` this is the full CSRF defence — there is no synchronizer token.

**First-user bootstrap.** When the `users` table is empty, `/auth/login` 302s to `/auth/setup`, and `/auth/setup` POST creates the first user as `UserRole::Admin` and signs them in. Subsequent users are admin-created via `/users/new`. The E2E `support/seeding.js` mirrors this flow.

**Server-rendered, classic POST→Redirect.** Templates are Askama (`templates/`), one struct per template. Success paths `Redirect::to(...)`, error paths re-render the template with an `error: Option<String>` field. There's no JSON API.

**Destructive actions confirm on the server; `window.confirm()` is only an enhancement on top.** An `onsubmit="return confirm(…)"` guard does nothing with JavaScript off — the form just posts, and the deed is done unannounced. So every destructive route is registered as `get(confirm_page).post(action)` on one path: the trigger is an `<a href>`, the GET renders an interstitial via `handlers::confirm::page` (`templates/confirm.html`), and only its POST acts. The GET must be inert and must apply the *same* ownership check as the POST — each has a test asserting both.

Where scripts do run, a delegated handler in `base.html` intercepts clicks on `a[data-confirm]`, asks in a dialog, and POSTs to the same URL — so JS users keep the one-click feel and never see the interstitial. A destructive trigger therefore needs **both** the `href` and the `data-confirm`; dropping either silently costs one audience its confirmation or its speed. The dialog text is the short question, the page keeps the detail (how many sets cascade, how many devices sign out) because only the server knows those counts. `handlers::auth`'s promote/delete-user pages are the same route shape plus an admin password re-check, keep their own template, and are deliberately *not* enhanced — re-authentication needs a real form.

**Migrations are baked in.** `src/migrations.rs` `include_str!`'s every file in `migrations/` and applies them at startup, tracking applied versions in a `_migrations` table. Tests use `run_migrations_for_tests` against an in-memory pool. Filenames are gap-tolerant (numbers aren't contiguous) — append `NNN_description.sql` and add it to the `MIGRATIONS` slice in order.

**Exercise categories are code, not data.** `CATEGORIES` in `src/models/exercise.rs` is a `&'static` slice; exercises store the category as a string column constrained to those values. Adding/renaming a category is a code change, not a migration.

**Timestamps render in the browser's timezone, not the server's.** Every `DateTime<Utc>` shown in the UI is emitted as `<time datetime="{{ x.to_rfc3339() }}" data-fmt="datetime|date">{{ x.format("…UTC") }}</time>`; the server-rendered text is only the no-JS fallback. An inline script in `templates/base.html` exposes `window.LiftLog.formatLocalDate/formatLocalDateTime` and rewrites those nodes on `DOMContentLoaded` into a fixed `YYYY-MM-DD HH:MM GMT±H` (dates: `YYYY-MM-DD`) — deliberately not `toLocaleString()`, so column widths stay constant. `NaiveDate` columns (`workout.date`, chart x-axes) are user-entered calendar dates and must **not** be converted.

**Build script side-effects.** `build.rs` renders `apple-touch-icon.png` from `assets/favicon.svg` via `resvg` and stamps `GIT_VERSION` (from `git describe` or the `GIT_VERSION` env override used by Docker/CI) into the binary as a `rustc-env`.

## Integration test harness

`tests/common/mod.rs` exposes `setup_test_db()` (in-memory sqlite, fully migrated) and `create_test_app_with_session()` (router + a pre-seeded session). Every `tests/*_test.rs` file uses these — match that pattern for new tests rather than building a fresh server.

## E2E test harness

`npm test` runs `cargo build` once, then `bddgen && playwright test`. A worker-scoped fixture in `steps/fixtures.js` (`workerServer`) spawns one server per Playwright worker, on port `3100 + workerInfo.workerIndex` with sqlite at `tests/e2e/.tmp/liftlog-e2e-{idx}.sqlite3`; Playwright's `baseURL` fixture is overridden to point at that per-worker URL so steps stay worker-agnostic.

- **One DB per worker, not per run.** Workers run in parallel; within a worker, scenarios run sequentially. `workers: process.env.WORKERS ?? (CI ? 2 : '50%')` — override with `WORKERS=4 npm test`.
- **Scenario data is still scoped.** The `scenarioState` fixture assigns each scenario a random suffix; steps use `scenarioState.unique('Squat')` and assert only on what the scenario built. Don't assume "lifter has no other workouts".
- **`_bootstrap.feature` only needs its worker's DB empty at worker start.** Both scenarios are no-mutation, so they work no matter which worker they land on.
- **Confirm dialogs fire from a link, not a form.** Workout-delete, set-delete, exercise-delete, revoke-share and logout-others are `<a data-confirm>` links that `base.html` intercepts — the click still raises `window.confirm()`, so keep `page.once('dialog', d => d.accept())` before it, but target the **link**, not a button. Promote-user and delete-user are not enhanced: they open a page that re-checks the admin's password, so those steps click a link, fill the password, and submit.
- **The no-JS path is covered in Rust, not here.** The confirmation pages are asserted by the integration tests (right consequence, inert on GET, ownership enforced); a `javaScriptEnabled: false` scenario was tried and hung in CI, and a real browser adds little over those beyond proving an `<a href>` navigates. When changing a destructive trigger, keep the Rust assertions on both the `href` and the `data-confirm`.
- **Guest views.** Public share URLs are tested via `browser.newContext()` so the logged-in cookie doesn't leak in.
- **HTML form validation can swallow the request.** Every password field carries `minlength`/`maxlength`, so any scenario submitting a deliberately-invalid password sets `form.noValidate = true` first (the setup step in `auth.steps.js`, `fillPasswordForm` in `settings.steps.js`); otherwise the browser blocks it client-side and the server-side defense — which is the actual control — goes untested.
- **Keep Playwright in step with the other repos sharing the browser cache.** `~/.cache/ms-playwright` (macOS: `~/Library/Caches/ms-playwright`) is shared by every checkout, and each `playwright-core` pins one exact chromium revision. A lagging repo misses the cache and falls back to a CDN download with no wall-clock timeout, and `playwright install` garbage-collects revisions no registered checkout references — so its browser gets deleted and re-downloaded repeatedly. Bump alongside the other repos; `npm` is in `dependabot.yml` (7-day cooldown).

## Project conventions

- Commits follow conventional-commits with an area scope: `feat(stats):`, `fix(workouts):`, `chore(deps):`, `test(e2e):`, `refactor(auth):`. PR titles mirror the commit subject.
- GitHub Actions are pinned by SHA with the human tag as a trailing comment.
- `MSRV` (`rust-version` in `Cargo.toml`) is managed independently of the toolchain — don't bump it when bumping the toolchain.
- Release artifacts are cut via `gh release create --generate-notes`; `Cargo.toml` version and `CHANGELOG.md` are not edited by hand.
