# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

LiftLog is a self-hosted workout journal: an Axum + Askama server-rendered app backed by a single SQLite file, shipped as a Docker image.

## Commands

```bash
cargo run                                 # dev server on $LIFTLOG_BIND (default 127.0.0.1:8080)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo deny check
cargo nextest run
cargo nextest run --test workout_test     # single integration file
cargo nextest run -p liftlog session_repo # filter by name
```

E2E suite (cucumber + thirtyfour, in `e2e/`, its own workspace):

```bash
cargo build                               # first — the suite never rebuilds a stale binary
cd e2e
cargo test --test e2e
cargo test --test e2e -- -n "Add Set"     # one scenario (regex)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Needs a local Chrome/Chromium (`brew install --cask ungoogled-chromium`); the driver manager fetches chromedriver but never the browser. No Node anywhere.

## Architecture

**State.** `AppState` (`src/state.rs`) holds the four repositories (user, exercise, workout, session) over an `r2d2` SQLite pool, plus rate limiters and config. Handlers take `State<AppState>`.

**Sessions.** `sliding_session_middleware` (`src/middleware/auth.rs`) runs on every route: it calls `SessionRepository::validate_and_touch` and injects a `ValidatedSession` extension. `AuthUser`/`AdminUser` extractors read that extension, never the DB. Routes that must not refresh the cookie (logout) insert `SuppressSessionRefresh`. A tokio task in `main.rs` sweeps expired sessions hourly.

**CSRF.** `tower_http::csrf::CsrfLayer` is outermost (registered after the session layer, so it runs first) and 403s cross-site state-changing requests. `Sec-Fetch-Site` decides when present — only `same-origin`/`none` pass, **not** `same-site`. Otherwise `Origin`'s authority, **port included**, must match the request's. Safe methods and header-less clients (curl, tests) pass. With `SameSite=Lax` cookies this is the entire CSRF defence; there is no token.

Keep the port check. Cookies ignore ports, so a port-blind check (#187, reverted) would treat any other service on the same host as same-origin. On plain HTTP there is no fetch metadata, so the `Origin` check is the only one running. Operators forward `Host` with its port (nginx `$http_host`), as the README says.

`src/middleware/csrf.rs` holds `log_csrf_rejection`, layered just outside `CsrfLayer`, which emits the `csrf.rejected` audit event (`reason` = `sec_fetch_site` or `origin_fallback`) from the `ProtectionError` on the 403.

**First-user bootstrap.** With no users, `/auth/login` redirects to `/auth/setup`, whose POST creates an admin and signs in. Later users are created by an admin at `/users/new`. `e2e/src/seeding.rs` does the same over HTTP, idempotently.

**Server-rendered POST→Redirect.** One Askama struct per template (`templates/`). Success redirects; errors re-render with `error: Option<String>`. No JSON API.

**Destructive actions confirm on the server.** Each is `get(confirm_page).post(action)` on one path: the trigger is an `<a href>`, the GET renders `handlers::confirm::page` (`templates/confirm.html`), and only the POST acts. The GET must be inert and apply the **same ownership check** as the POST; each has a test for both. With JS, a delegated handler in `base.html` intercepts `a[data-confirm]`, shows `confirm()`, and POSTs directly. So a trigger needs **both** `href` and `data-confirm`. The dialog asks the short question; the page gives detail only the server knows (cascade counts etc.). Promote/delete-user use the same route shape plus an admin password re-check with their own template, and are deliberately not JS-enhanced.

**Migrations** are `include_str!`'d in `src/migrations.rs` and applied at startup (tracked in `_migrations`). Add `NNN_description.sql` (gaps are fine) and append it to `MIGRATIONS`. Tests use `run_migrations_for_tests`.

**Exercise categories are code.** `CATEGORIES` in `src/models/exercise.rs`; changing them is a code change, not a migration.

**Timestamps render in the browser's timezone.** Emit every `DateTime<Utc>` as `<time datetime="{{ x.to_rfc3339() }}" data-fmt="datetime|date">{{ x.format("…UTC") }}</time>`; the text is the no-JS fallback. `base.html` rewrites these to a fixed `YYYY-MM-DD HH:MM GMT±H` (or `YYYY-MM-DD`) via `window.LiftLog.formatLocalDate/formatLocalDateTime` — not `toLocaleString()`, to keep column widths constant. `NaiveDate`s (`workout.date`, chart x-axes) are calendar dates and must **not** be converted.

**The progress chart is drawn twice; keep both in sync.** `/stats/exercise/{id}?metric=top_set|e1rm|volume&range=20|all` is rendered as SVG by `handlers::stats::render_chart`; tabs are plain links. The script in `templates/stats/exercise.html` intercepts them, redraws from the embedded `ChartPoint` JSON, and `history.replaceState`s the URL. `ChartMetric::value` mirrors `metricValue`; PR dots are the running best of the plotted series on both sides; the server's `<g id="chart-hit-areas">` bands (with SVG `<title>` tooltips) mirror the client's. Unknown query values fall back to defaults.

**Add Set is server-prefillable.** Clone links to `/workouts/{id}?prefill=<log_id>`; `show` resolves it only against the workout's own logs. The script fills the form in place instead, so the trigger has both `href` and `data-clone-*`. The "last weight" hint is also rendered in `<noscript>`.

**JS-only controls ship `hidden`.** The share page's clipboard button has `hidden data-requires-js`; `base.html` reveals it on `DOMContentLoaded`. `[hidden] { display: none !important }` exists because `.btn`'s `display: inline-flex` would otherwise win.

**`build.rs`** renders `apple-touch-icon.png` from `assets/favicon.svg` (resvg) and sets `GIT_VERSION` from `git describe` or the `GIT_VERSION` env var (Docker/CI).

## Integration tests

Use `tests/common/mod.rs`: `setup_test_db()` (in-memory, migrated) and `create_test_app_with_session()` (router + seeded session). Don't build a fresh server.

## E2E tests

`e2e/tests/e2e/main.rs` starts one `target/debug/liftlog` on a throwaway SQLite file and OS-assigned port, opens one browser per scenario, and kills the server afterwards.

- **Stale binary.** `ensure_binary` (`e2e/src/server.rs`) builds only if the binary is missing. Askama compiles templates in, so run `cargo build` at the root after any `src/` or `templates/` change, or the suite tests the old build. CI builds first.
- **Separate workspace.** Nothing is inherited: the lint set is copied into `e2e/Cargo.toml` (keep in sync). It's excluded from `cargo deny` and `.dockerignore`d.
- **One server, one DB for the run.** Isolate fixtures with a per-scenario suffix (`world.unique("Squat")`); never assume a user has no other data.
- **`@bootstrap` runs first**, as a separate pass on the empty DB. The tag is on the feature, and `gherkin` doesn't propagate feature tags, so the filter checks both.
- **Concurrency** is `available_parallelism` capped at 4; `WAIT_TIMEOUT` is 30s — sized for a two-core CI runner.
- **Wait for every submit's effect** (new URL, row appears/disappears). `click` may return before the redirect, and the next navigation cancels the in-flight POST, silently losing the fixture.
- **Confirm dialogs** are auto-accepted via `unhandledPromptBehavior: accept`. Promote/delete-user steps fill the password page instead.
- **No-JS paths are tested in Rust**, not here. When changing a destructive trigger, keep the Rust assertions on both `href` and `data-confirm`.
- **Status codes and guest access go over HTTP** (`e2e/src/http.rs`), since WebDriver can't see responses — with the browser's cookie for 403/404, without for share links.
- **`WebElement::text()` is rendered text** (affected by `text-transform`); compare user-chosen names with `pages::dom_text` (`textContent`).
- **Set `noValidate` before submitting invalid passwords** (`SetupPage::submit`, `SettingsPage::change_password`), or `minlength`/`maxlength` blocks the request client-side.
- **`Browser::prepare`** runs one session first so parallel sessions don't race to download the driver.

## Conventions

- Conventional commits with an area scope (`feat(stats):`, `fix(workouts):`, `chore(deps):`, `test(e2e):`); PR titles match.
- GitHub Actions pinned by SHA with the tag as a trailing comment.
- Don't bump `rust-version` (MSRV) along with the toolchain.
- Releases: `gh release create --generate-notes`; never edit `Cargo.toml` version or add `CHANGELOG.md`.
