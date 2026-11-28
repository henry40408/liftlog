# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

LiftLog is a self-hosted workout journal: a Go `net/http` server-rendered app backed by a single SQLite file (pure-Go `modernc.org/sqlite`, no cgo), shipped as a Docker image. It was rewritten from Rust; some fixtures and comments refer to "the Rust build" for that reason.

## Commands

The Go version comes from `go.mod` alone (`go` + `toolchain` lines). Third-party modules are **vendored and committed** (`vendor/`, and `e2e/vendor/`); Go selects `-mod=vendor` automatically.

```bash
go build -o bin/liftlog ./cmd/liftlog     # the binary the e2e suite runs
go run ./cmd/liftlog                      # dev server on $LIFTLOG_BIND (default 127.0.0.1:8080)
go test -race ./...
go test ./internal/server -run TestCSRF   # one test
golangci-lint fmt --diff ./...             # `run` does not check formatting, so this is separate
golangci-lint run ./...                   # config in .golangci.yml; suppress with `//nolint:<linter> // reason`
go run golang.org/x/vuln/cmd/govulncheck@v1.9.0 ./...
# licences (CI-enforced; GOROOT must be set explicitly or the standard library fails with "does not have module info")
GOROOT=$(go env GOROOT) go run github.com/google/go-licenses/v2@v2.0.1 check ./... --include_tests --allowed_licenses=<list from ci.yml>
```

Licences are enforced by the `deps` job in `.github/workflows/ci.yml`, which holds the allow-list (`ALLOWED_LICENSES`). The root module and `e2e/` are each checked once; `e2e/` alone also allows MPL-2.0 (godog pulls in go-immutable-radix, go-memdb and golang-lru), and MPL-2.0 must not enter the root module. Locally, run the `e2e/` check from `e2e/` with `--ignore github.com/henry40408/liftlog/e2e` (the module has no licence file) and `,MPL-2.0` appended. `--include_tests` is needed to see the test-only dependencies. An Unknown or unlisted licence is a decision for the maintainer: do not widen the list or swap the module to get around it.

Lint is golangci-lint (config in `.golangci.yml`; run it in `e2e/` too, `./...` does not enter that module). CI installs it with `golangci/golangci-lint-action` (`install-only`), so the linter release floats with the action and dependabot tracks only the action. `//lint:ignore` is not honoured: use `//nolint:<linter> // reason`.

CI also checks: `golangci-lint fmt --diff` prints nothing; `go mod tidy -diff`; after `go mod vendor`, `git status --porcelain -- vendor go.mod go.sum` prints nothing (`git diff` would miss an untracked vendored package). To add or update a dependency: `go get <module>@<version>`, `go mod tidy`, `go mod vendor`, and commit `go.mod`, `go.sum` and `vendor/` together. Check the release is at least 7 days old first.

Coverage (as CI): `LIFTLOG_COVERDIR=$PWD/covdata go test -coverprofile=coverage.out ./...`, then `go tool covdata textfmt -i=covdata -o coverage-bin.out` and append it without its `mode:` line. `-coverprofile` ignores the binary `cmd/liftlog`'s `TestMain` builds and execs; with `LIFTLOG_COVERDIR` set it builds that binary with `-cover`.

Mutation testing (CI-enforced, the single `mutation` job): `go clean -testcache`, then `go run github.com/go-gremlins/gremlins/cmd/gremlins@v0.6.0 unleash .`. The efficacy threshold (the run fails when efficacy is 95% or below), `timeout-coefficient` and the exclusions live in `.gremlins.yaml`. **Never pass `--threshold-efficacy` on the command line**: gremlins v0.6.0 reads a float64 flag through viper as a string, gets 0 ("no threshold") and always exits 0; the same value from the yaml works. Pushes to `main` run this full run (about 8 minutes on 10 cores; CI's 4-vCPU runner is slower, expect 20-25 minutes, hence `timeout-minutes: 45`). Pull requests run it incrementally: add `--diff origin/main` (needs full git history; `git diff --merge-base` runs underneath) and gremlins mutates only the changed lines, taking seconds to minutes. Gremlins reads an empty diff as "everything changed" and would run the whole thing, so CI skips the step when `git diff --merge-base --quiet origin/main` says nothing changed. A diff with no mutable Go lines (docs, tests only) yields zero mutants and a 0% score, which fails the threshold; the CI pull request step detects zero mutants in the output and treats it as a pass. `timeout-coefficient: 20` (in `.gremlins.yaml`) keeps slow handler tests from timing out under parallel workers; on a machine with many cores, lower `--workers` (default: all cores) if mutants still time out. Clean the test cache first: gremlins derives each mutant's timeout from its coverage run, a cached run makes that near zero, and most mutants then "time out", which the score ignores. `e2e/` is excluded because it is a separate module with no coverage profile, so all its mutants count as "not covered"; `vendor/` is third-party code. Gremlins only mutates covered code, so it complements the coverage number. A surviving `CONDITIONALS_BOUNDARY` mutant usually means a missing "exactly at the limit" test; a `TIMED OUT` one needs no test only if the mutation really hangs (an endless loop, say); otherwise rerun with a higher `timeout-coefficient`, inspect the result, and treat a mutant that then lives like any other survivor.

E2E (godog + chromedp, in `e2e/`, its own module with its own `vendor/`; `go test ./...` at the root does not run it):

```bash
go build -o bin/liftlog ./cmd/liftlog     # first, the suite never rebuilds an existing binary
cd e2e
go test -count=1 -v ./...                 # -count=1: the test cache cannot see a rebuilt binary
E2E_FEATURES=features/auth/login.feature go test -count=1 -v ./...   # comma-separated subset
golangci-lint fmt --diff ./... ; golangci-lint run ./...
```

Needs a local Chrome/Chromium (`brew install --cask ungoogled-chromium`); chromedp starts it itself, so there is no driver to download. `E2E_CHROME` pins the browser binary (CI sets `/usr/bin/google-chrome`). `E2E_BINARY` pins the server binary. No Node anywhere.

README screenshots: `cd e2e && go run -mod=vendor ./cmd/screenshots` (writes `../screenshots/`; build `bin/liftlog` first, same reason as above).

## UI changes require screenshot updates

Any change to rendered UI (`internal/views/`, `internal/assets/static/`): rebuild `bin/liftlog`, run the generator above and include the four updated images under `screenshots/` (embedded by `README.md`) in the same change. The generator (`e2e/cmd/screenshots`) captures the dashboard and a lift's progress page, each on a wide viewport and on a phone. The app has only a dark theme, so there are no `-dark` variants.

## Architecture

**Layout.** A thin `cmd/liftlog` (flags, `.env`, logging, serve, hourly sweep) over `internal/` packages, so tests build the handler directly: `server.New(options).Handler()`. `internal/server` holds routing, middleware and handlers; `internal/repo` the SQL (one type per table, context + `*sql.DB`); `internal/views` the pages; `internal/chart` the progress chart; `internal/auth` password hashing, policy, rate limits, backoff, setup token and client IP; `internal/session` the sliding-expiry rules and cookie; `internal/audit` the audit events; `internal/timezone` the allowed zone names; `internal/db` opening SQLite and the time/id storage conventions; `internal/assets` the embedded CSS/JS/icons; `migrations/` the SQL files. `internal/zxcvbn` is a rune-correct fork of trustelem/zxcvbn that follows the Rust crate's verdicts.

**State.** `server.App` (`internal/server/server.go`, built by `server.New(Options)`) holds the repositories, the audit logger, the throttles and the config. Handlers are methods on `*App`.

**Sessions.** `sessionMiddleware` (`internal/server/middleware_session.go`) runs on every route: it calls `Sessions.ValidateAndTouch` (the rules live in `session.ComputeTouchAction`) and stores a `ValidatedSession` in the request state. `requireUser`/`requireAdmin` (`context.go`) read that state, never the DB. Routes that must not refresh the cookie (logout, the password-change rotation) call `suppressSessionRefresh`. A goroutine started from `cmd/liftlog/sweep.go` sweeps expired sessions hourly. Tokens are opaque and DB-backed, so revocation is instant; JWT/refresh tokens were rejected as complexity without benefit. The active-session list stores no User-Agent or IP, by choice. The cookie is `session`, or `__Host-session` when `LIFTLOG_COOKIE_SECURE` is true.

**CSRF.** `csrfGuard` (`internal/server/middleware_csrf.go`) wraps `http.NewCrossOriginProtection()` and 403s cross-site state-changing requests with an empty body. Order, outermost first, in `Handler` (`routes.go`): HSTS + baseline headers (`securityHeaders`) → CSRF guard (which also emits the audit event) → session → mux. `Sec-Fetch-Site` decides when present — only `same-origin`/`none` pass, **not** `same-site`. Otherwise `Origin`'s authority, **port included**, must match the request's `Host`. Safe methods and header-less clients (curl, tests) pass. With `SameSite=Lax` cookies this is the entire CSRF defence; there is no token.

Keep the port check. Cookies ignore ports, so a port-blind check would treat any other service on the same host as same-origin. On plain HTTP there is no fetch metadata, so the `Origin` check is the only one running. Operators forward `Host` with its port (nginx `$http_host`), as the README says.

The guard emits the `csrf.rejected` audit event (`reason` = `sec_fetch_site` or `origin_fallback`, from `csrfReason`) on every 403.

**Audit events** are `slog` records with the attribute `target=liftlog::audit` and an `event` name (`internal/audit/audit.go`); the logger is injected, so tests capture them. Field names are part of the operator-facing contract in the README.

**First-user bootstrap.** With no users, `/auth/login` redirects to `/auth/setup`, whose POST creates an admin and signs in (`Users.CreateFirstAdmin` is one atomic `INSERT … WHERE NOT EXISTS`). The form needs a **setup token** that the server prints once on **stderr only** (never through the structured log) while no user exists. Later users are created by an admin at `/users/new`. `e2e/internal/harness/seeding.go` does the same over HTTP, idempotently, with the token it read from the server's stderr.

**Server-rendered POST→Redirect.** One view function per page in `internal/views/` (shared layout in `layout.go`); markup is built with the small escaping builder in `html.go` (not `html/template`; it escapes only `& < > "`). Success redirects with 303; errors re-render with an error string. No JSON API except `/health`.

**Destructive actions confirm on the server.** Each is a GET + POST on one path: the trigger is an `<a href>`, the GET renders `writeConfirmPage` (`internal/server/confirm.go`), and only the POST acts. The GET must be inert and apply the **same ownership check** as the POST; each has a test for both. With JS, a delegated handler in `internal/assets/static/app.js` intercepts `a[data-confirm]`, shows `confirm()`, and POSTs directly. So a trigger needs **both** `href` and `data-confirm`. The dialog asks the short question; the page gives detail only the server knows (cascade counts etc.). Promote/delete-user use the same route shape plus an admin password re-check (`reauth.go`) with their own page, and are deliberately not JS-enhanced.

**Migrations** are `go:embed`ded SQL files in `migrations/`, named `<version>_<description>.sql` (gaps are fine) and applied at startup by `migrations.Run`; tests use `migrations.RunForTests`. `PRAGMA user_version` records the version, forward-only. Each file runs in one transaction together with the version bump. `0013_baseline.sql` is the whole schema of version 13, where releases up to 0.31 ended, so lower versions have no file. A non-empty database below 13 is accepted only if `_migrations` has the `013_add_user_timezone.sql` row; a database newer than the latest file is refused.

Rules for a new file:

- Never edit a released file; add one with the next number.
- Make it safe to run twice where SQLite allows it: `CREATE` and `DROP` take `IF (NOT) EXISTS`, and a data change leaves the data alone the second time. A restored `.dump` gets its version back by hand and may get it too low. `ADD COLUMN` has no such guard: let it fail.
- No `BEGIN` or `COMMIT`; the runner owns the transaction.
- Leave `_migrations` alone: releases up to 0.31 read it, and the baseline check relies on it.
- Name the columns when copying rows. Databases created before 0.4 have extra columns in `exercises` and `workout_logs`, and a `personal_records` table.
- Rebuilding a table that foreign keys point to (`users`, `exercises`, `workout_sessions`) is dangerous. Foreign keys stay on inside the transaction (`PRAGMA foreign_keys` has no effect there), so `DROP TABLE` cascades into child rows without an error, or fails on `ON DELETE RESTRICT`. Preserve the child rows in the same file.

**Storage conventions** (`internal/db`). Timestamps are written as text in chrono's format (`2006-01-02 15:04:05[.fraction]+00:00`) and read back from that, RFC 3339, or the bare form, always as UTC. Always bind strings, never `time.Time`. Dates are `YYYY-MM-DD`. An in-memory test database is limited to one connection.

**Exercise categories are code.** `models.Categories` in `internal/models/exercise.go`; changing them is a code change, not a migration.

**Timestamps render in the browser's timezone.** Emit every instant through `views` (`shared.go`) as `<time datetime="…RFC3339…" data-fmt="datetime|date">…UTC text…</time>`; the text is the no-JS fallback. `app.js` rewrites these to a fixed `YYYY-MM-DD HH:MM GMT±H` (or `YYYY-MM-DD`) via `window.LiftLog.formatLocalDate/formatLocalDateTime` — not `toLocaleString()`, to keep column widths constant. Plain calendar dates (`workout.date`, chart x-axes) must **not** be converted. User timezones are validated against `internal/timezone/zones.txt` (exact, case-sensitive).

**The progress chart is drawn twice; keep both in sync.** `/stats/exercise/{id}?metric=top_set|e1rm|volume&range=20|all` is rendered as SVG by `internal/chart` (`chart.Render`, parsed with `chart.ParseMetric`/`ParseRange`); tabs are plain links. The script `internal/views/scripts/stats_exercise.js` intercepts them, redraws from the embedded point JSON (`chart.EncodeData`), and `history.replaceState`s the URL. `Metric.Value` mirrors `metricValue`; PR dots are the running best of the plotted series on both sides; the server's `<g id="chart-hit-areas">` bands (with SVG `<title>` tooltips) mirror the client's. Unknown query values fall back to defaults. The JSON must print floats like Rust did (`100.0`); see `chart/format.go`.

**Add Set is server-prefillable.** Clone links to `/workouts/{id}?prefill=<log_id>`; `prefillFrom` (`handlers_workouts.go`) resolves it only against the workout's own logs. The script (`workout_show.js`) fills the form in place instead, so the trigger has both `href` and `data-clone-*`. The "last weight" hint is also rendered in `<noscript>`.

**JS-only controls ship `hidden`.** The share-link clipboard button (`internal/views/workout_show.go`, the owner's workout page; the public shared page has none) has `hidden data-requires-js`; `app.js` reveals it on `DOMContentLoaded`. `[hidden] { display: none !important }` exists because `.btn`'s `display: inline-flex` would otherwise win.

**Version.** `internal/version.Version()` returns the `-ldflags -X …/internal/version.version=` value (Docker builds pass it from the `GIT_VERSION` build arg because `.dockerignore` excludes `.git`), else the VCS-stamped module version, else `dev`. `apple-touch-icon.png` is a committed asset (`internal/assets/static/`); there is no build step.

**Configuration and logging.** `internal/config` parses the environment; retired variables (`BIND`, `LOG_FORMAT`, `RUST_LOG`) fail startup (`config/legacy.go`: update that list whenever a variable is added, renamed or removed). `cmd/liftlog` reads `./.env` only (strict: a malformed line is fatal; never overrides a set variable), then flags > environment > default for `--log-format` (`full`|`json`) and `--log-level`. Logs go to stdout; stderr is reserved for fatal errors and the setup token.

## Tests

- **Server tests** use `internal/server/harness_test.go` (in-memory migrated DB, the real `Handler`, seeded users and sessions). Don't build a fresh server.
- **No `testdata` directories.** Everything the Rust build produced lives in the tests as Go values; the original files are in git history (`git log --diff-filter=D -- <old path>`).
- **Golden pages.** `goldenDigests` (`internal/views/golden_digests_test.go`) holds one sha256 per page the Rust server rendered, because the e2e suite and the no-JS CSS depend on that exact markup. `assertPageGolden` (`golden_test.go`) hashes the Go output after `normaliseVersion` maps the build version back to `capturedVersion` and `normaliseWeightStep` maps the weight field's deliberately finer `step` back to Rust's; the clock comes from `wkClock` and the database from `wkSeedSQL` (`seed_test.go`). `login-shell` and `dashboard-nav` pin a cut of a page, not a whole one. A mismatch writes the page to a temp file and prints the `git show` command that recovers the Rust golden. Never recompute a digest to make a test pass.
- **Other fixtures from the Rust build**: `rustCorpus` (`internal/zxcvbn/rust_corpus_test.go`, passwords scored by Rust zxcvbn 3.1.1), `rustPolicyCases` (`internal/auth/rust_policy_cases_test.go`, the password policy's verdicts and messages), `rustArgon2Hashes` (`internal/auth/password_test.go`), and `openRustDB` (`internal/repo/rust_created_test.go`), which rebuilds the Rust-created database from the embedded migrations plus the captured rows.
- **Old schema.** `release031Schema` (`migrations/legacy_schema_test.go`) is the schema releases up to 0.31 left in a database created by 0.4 or later; the baseline is compared against it, so never regenerate it from the baseline. `pre04Tables` beside it is what a database created earlier still has instead; test a migration that touches those tables against both.
- `cmd/liftlog/main_test.go` builds the binary in `TestMain` and execs it; use that shape for anything waiting on a log line or a signal.

## E2E tests

`e2e/e2e_test.go` starts one `bin/liftlog` on a throwaway SQLite file and OS-assigned port, opens one Chrome per scenario, and stops the server afterwards.

- **Stale binary.** `ensureBinary` (`e2e/internal/harness/server.go`) builds only if the binary is missing. Run `go build -o bin/liftlog ./cmd/liftlog` after any server change, or the suite tests the old build. CI builds first.
- **Separate module.** Its own `go.mod`/`vendor/`; the `go` and `toolchain` lines must equal the root's (CI checks). It is `.dockerignore`d.
- **One server, one DB for the run.** Isolate fixtures with a per-scenario suffix (`world.unique("Squat")`); never assume a user has no other data.
- **`@bootstrap` runs first**, as a separate pass on the empty DB. The tag is on the feature; the filter checks feature and scenario tags (pickles carry both).
- **Concurrency** is `min(NumCPU, 4)`; `harness.WaitTimeout` is 30s — sized for a two-core CI runner.
- **Wait for every submit's effect** (new URL, row appears/disappears). `click` may return before the redirect, and the next navigation cancels the in-flight POST, silently losing the fixture.
- **Confirm dialogs** are auto-accepted. Promote/delete-user steps fill the password page instead.
- **`@no-js`** (on a feature or a scenario) runs with scripting off via CDP `Emulation.setScriptExecutionDisabled`. `features/no_js/` drives the destructive-action confirmation pages that way. The handler-level Go tests still assert both `href` and `data-confirm` on every destructive trigger; keep those when changing one.
- **Status codes and guest access go over HTTP** (`harness/http.go`), since the browser can't see responses — with the browser's cookie for 403/404, without for share links.
- **Rendered text vs `textContent`.** Compare user-chosen names with `Browser.DomText` (`textContent`), not rendered text, which `text-transform` changes.
- **Set `noValidate` before submitting invalid passwords** (`pages.go`, used by setup and change-password), or `minlength`/`maxlength` blocks the request client-side.
- Features live in `e2e/features/**`.

## Conventions

- Conventional commits with an area scope (`feat(stats):`, `fix(workouts):`, `chore(deps):`, `test(e2e):`); PR titles match.
- GitHub Actions pinned by SHA with the tag as a trailing comment.
- Dependencies, actions and base images must be at least 7 days old.
- Format with `golangci-lint fmt ./...` (gofmt) before committing.
- Releases: `gh release create --generate-notes`; never add a `CHANGELOG.md`.
