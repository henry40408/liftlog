# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

LiftLog is a self-hosted workout journal: an Axum + Askama server-rendered app backed by a single SQLite file, shipped as a Docker image.

## Commands

```bash
cargo run                                 # dev server on $BIND (default 127.0.0.1:8080)
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

**First-user bootstrap.** When the `users` table is empty, `/auth/login` 302s to `/auth/setup`, and `/auth/setup` POST creates the first user as `UserRole::Admin` and signs them in. Subsequent users are admin-created via `/users/new`. The E2E `support/seeding.js` mirrors this flow.

**Server-rendered, classic POST→Redirect.** Templates are Askama (`templates/`), one struct per template. Forms POST to the same handler shape; success paths `Redirect::to(...)`, error paths re-render the template with an `error: Option<String>` field. There's no JSON API.

**Migrations are baked in.** `src/migrations.rs` `include_str!`'s every file in `migrations/` and applies them at startup, tracking applied versions in a `__schema_migrations` table. Tests use `run_migrations_for_tests` against an in-memory pool. Migration filenames are gap-tolerant (numbers aren't contiguous) — just append `NNN_description.sql` and add it to the `MIGRATIONS` slice in order.

**Exercise categories are code, not data.** `CATEGORIES` in `src/models/exercise.rs` is a `&'static` slice; exercises store the category as a string column constrained to those values. Adding/renaming a category is a code change, not a migration.

**Build script side-effects.** `build.rs` renders `apple-touch-icon.png` from `assets/favicon.svg` via `resvg` and stamps `GIT_VERSION` (from `git describe` or the `GIT_VERSION` env override used by Docker/CI) into the binary as a `rustc-env`.

## Integration test harness

`tests/common/mod.rs` exposes `setup_test_db()` (in-memory sqlite, fully migrated) and `create_test_app_with_session()` (router + a pre-seeded session). Every `tests/*_test.rs` file uses these — match that pattern for new tests rather than building a fresh server.

## E2E test harness

`npm test` (`tests/e2e/`) runs `cargo build` once for the shared workspace target, then `bddgen && playwright test`. A worker-scoped fixture in `steps/fixtures.js` (`workerServer`) spawns one server per Playwright worker, on port `3100 + workerInfo.workerIndex` with sqlite at `tests/e2e/.tmp/liftlog-e2e-{idx}.sqlite3`; Playwright's built-in `baseURL` fixture is overridden to point at that per-worker URL so steps stay worker-agnostic. Server processes are killed on worker teardown.

- **One DB per worker, not per run.** Each worker boots with a fresh sqlite. Workers run in parallel; within a worker, scenarios still run sequentially.
- **Worker count.** `workers: process.env.WORKERS ?? (CI ? 2 : '50%')`. Override locally with `WORKERS=4 npm test`.
- **Scenario data is still scoped.** The `scenarioState` fixture assigns each scenario a random suffix; steps use `scenarioState.unique('Squat')` to name entities and assert only on what the scenario built. Don't assume "lifter has no other workouts" — multiple scenarios on the same worker may have created some.
- **`_bootstrap.feature` only depends on its worker's DB being empty when the worker starts.** Both its scenarios are no-mutation (navigation; setup form with a 5-char password that fails server validation), so they leave DB state untouched and work no matter which worker they land on.
- **Confirm dialogs.** Workout-delete, set-delete, exercise-delete, revoke-share, promote-user, delete-user all use `window.confirm()` — handle with `page.once('dialog', d => d.accept())` before the click.
- **Guest views.** Public share URLs are tested via `browser.newContext()` so the logged-in cookie doesn't leak in.
- **HTML form validation can swallow the request.** The setup short-password scenario calls `form.noValidate = true` before submit so the request actually reaches the server; otherwise `minlength="6"` blocks it client-side and the server-side defense is untested.
- **Playwright is pinned to `~1.59.1`.** Playwright 1.60 moved internal paths that `playwright-bdd@8.5` still imports; don't bump without verifying the import surface.

## Project conventions

- Commits follow conventional-commits with an area scope: `feat(stats):`, `fix(workouts):`, `chore(deps):`, `test(e2e):`, `refactor(auth):`. PR titles mirror the commit subject.
- GitHub Actions are pinned by SHA with the human tag as a trailing comment.
- `MSRV` (`rust-version` in `Cargo.toml`) is managed independently of the toolchain — don't bump it when bumping the toolchain.
- Release artifacts are cut via `gh release create --generate-notes`; `Cargo.toml` version and `CHANGELOG.md` are not edited by hand.
