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

UI BDD suite (cucumber + thirtyfour, lives in `e2e/` — its own workspace):

```bash
cd e2e
cargo test --test e2e                     # headless; boots target/debug/liftlog itself
cargo fmt --all -- --check                # e2e inherits nothing from the root workspace
cargo clippy --all-targets -- -D warnings
```

A local Chrome or Chromium is a prerequisite (`brew install --cask ungoogled-chromium`);
thirtyfour's driver manager downloads a matching chromedriver itself but never the
browser. There is no Node anywhere in this repository.

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

**The progress chart is drawn twice, and the two must agree.** `/stats/exercise/{id}` takes `?metric=top_set|e1rm|volume` and `?range=20|all`; `handlers::stats::render_chart` draws that combination as server-rendered SVG, and the tabs are links to the same page with a different query string, so every series is reachable with scripts off. The inline script in `templates/stats/exercise.html` then intercepts those clicks and redraws from the embedded `ChartPoint` JSON, calling `history.replaceState` so the URL still describes what is on screen. Because both sides compute the geometry independently, changing one means changing the other: `ChartMetric::value` mirrors `metricValue`, the gold "PR" dots are a running best *of the plotted series* in both, and the server's `<g id="chart-hit-areas">` bands mirror the client's — each carrying an SVG `<title>` the browser shows as a native tooltip, which is how the figures stay readable on hover with no scripting. Unknown query values fall back to the defaults rather than erroring — the query string is user-editable and arrives from stale bookmarks.

**The Add Set form is prefillable from the server.** Clone is a link to `/workouts/{id}?prefill=<log_id>`; `show` resolves that id against the session's *own* logs (so an id from another workout simply fails to match) and seeds the exercise `<select>` and the weight/reps/RPE inputs. The page's script intercepts the click and fills the form in place instead, which is why the trigger carries both an `href` and `data-clone-*`. The inline "last weight" hint needs the `<select>` to change, which nothing can drive with scripts off, so the same figures are also rendered as a `<noscript>` table.

**A control that cannot work without scripts ships `hidden`.** The clipboard button on a shared workout is the only one — copying needs `navigator.clipboard`, so there is no server-side version to build. It carries `hidden data-requires-js` and the handler in `base.html` reveals it on `DOMContentLoaded`; a dead button that looks alive is worse than an absent one, and `<noscript>` cannot express "only when scripts run". `[hidden] { display: none !important }` is in the stylesheet because `.btn`'s `display: inline-flex` would otherwise outrank the UA rule.

**Build script side-effects.** `build.rs` renders `apple-touch-icon.png` from `assets/favicon.svg` via `resvg` and stamps `GIT_VERSION` (from `git describe` or the `GIT_VERSION` env override used by Docker/CI) into the binary as a `rustc-env`.

## Integration test harness

`tests/common/mod.rs` exposes `setup_test_db()` (in-memory sqlite, fully migrated) and `create_test_app_with_session()` (router + a pre-seeded session). Every `tests/*_test.rs` file uses these — match that pattern for new tests rather than building a fresh server.

## E2E test harness

`cargo test --test e2e` from `e2e/` runs the whole suite. `e2e/tests/e2e/main.rs` starts one `target/debug/liftlog` against a throwaway SQLite file on an OS-assigned port (building it first if it is missing), opens one browser per scenario, and kills the server on the way out. The `.feature` files are the Playwright suite's, reused verbatim apart from one tag.

- **`e2e/` is deliberately its own workspace.** Nothing is inherited across that boundary — the lint set is copied into `e2e/Cargo.toml` and drifts if you edit only the root. It is also outside `cargo deny` (the browser stack carries licences the server's allow-list does not, and none of it is shipped) and inside `.dockerignore`.
- **One server and one database for the whole run**, not one per worker. Scenarios are cucumber tasks on a single runtime, so they share the server and stay isolated the way they always did — by scoping fixtures to a per-scenario suffix (`world.unique("Squat")`). Never assume "lifter has no other workouts".
- **`@bootstrap` runs first, on the empty database.** The first-run scenarios assert on an install with no users, which stops being true the moment anything seeds its admin, so `main.rs` runs them as a separate pass before everything else. The tag is on the *feature*, and `gherkin` does not propagate feature tags onto scenarios — the filter checks both.
- **Concurrency is `available_parallelism` capped at 4**, and `WAIT_TIMEOUT` is 30s. Both are set for the slowest machine that runs this: four browsers on a two-core runner contend until pages settle slower than the steps wait for.
- **A form post is not finished when `click` returns.** WebDriver does not reliably block until a redirect has been followed, and the *next* navigation cancels the request still in flight — which shows up as a fixture that was silently never created. Every submit therefore waits for its own effect: the new URL, the row appearing, the entry leaving. When adding a step that posts, wait for something that only the completed write produces.
- **Confirm dialogs are handled by the session, not per click.** `unhandledPromptBehavior: accept` is set on the capabilities, so the `window.confirm()` that `base.html` raises for `<a data-confirm>` triggers is accepted automatically — there is no per-click handler to forget. Promote-user and delete-user are not enhanced: they open a page that re-checks the admin's password, so those steps click a link, fill the password, and submit.
- **The no-JS path is covered in Rust, not here.** The confirmation pages are asserted by the integration tests (right consequence, inert on GET, ownership enforced); a scripts-off scenario was tried in the Playwright suite and hung in CI, and a real browser adds little over those beyond proving an `<a href>` navigates. When changing a destructive trigger, keep the Rust assertions on both the `href` and the `data-confirm`.
- **Status codes and guests go over HTTP, not through the browser.** WebDriver reports the rendered document and nothing about the exchange, so `e2e/src/http.rs` re-issues the request — with the browser's session cookie for the 403/404 assertions, without one for the share-link guest.
- **`WebElement::text()` is *rendered* text.** An `<h1>` under `text-transform: uppercase` reports uppercase characters the document does not contain; use `pages::dom_text` (which reads `textContent`) when comparing against a name the scenario chose. XPath `normalize-space()` is unaffected — it reads the DOM.
- **HTML form validation can swallow the request.** Every password field carries `minlength`/`maxlength`, so any scenario submitting a deliberately-invalid password sets `noValidate` first (`SetupPage::submit`, `SettingsPage::change_password`); otherwise the browser blocks it client-side and the server-side defense — which is the actual control — goes untested.
- **The driver manager downloads the driver, never the browser.** A local Chrome or Chromium has to exist (`brew install --cask ungoogled-chromium` on macOS; GitHub's `ubuntu-latest` already ships Chrome), which is the one regression against Playwright, and why `Browser::open` names the prerequisite in its error. `Browser::prepare` runs one session up front so a cold driver cache is not downloaded by several sessions at once.

## Project conventions

- Commits follow conventional-commits with an area scope: `feat(stats):`, `fix(workouts):`, `chore(deps):`, `test(e2e):`, `refactor(auth):`. PR titles mirror the commit subject.
- GitHub Actions are pinned by SHA with the human tag as a trailing comment.
- `MSRV` (`rust-version` in `Cargo.toml`) is managed independently of the toolchain — don't bump it when bumping the toolchain.
- Release artifacts are cut via `gh release create --generate-notes`; `Cargo.toml` version and `CHANGELOG.md` are not edited by hand.
