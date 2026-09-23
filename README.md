# LiftLog

> A self-hosted workout logging application built with Rust.

[![CI](https://github.com/henry40408/liftlog/actions/workflows/ci.yml/badge.svg)](https://github.com/henry40408/liftlog/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/henry40408/liftlog/graph/badge.svg)](https://codecov.io/gh/henry40408/liftlog)
[![Release](https://img.shields.io/github/v/release/henry40408/liftlog)](https://github.com/henry40408/liftlog/releases/latest)
[![License](https://img.shields.io/github/license/henry40408/liftlog)](LICENSE.txt)
[![Rust toolchain](https://img.shields.io/badge/dynamic/toml?url=https://raw.githubusercontent.com/henry40408/liftlog/main/rust-toolchain.toml&query=$.toolchain.channel&label=rust%20toolchain&logo=rust)](https://www.rust-lang.org/)
[![Docker](https://img.shields.io/badge/docker-ghcr.io-blue.svg)](https://ghcr.io/henry40408/liftlog)
[![Casual Maintenance Intended](https://casuallymaintained.tech/badge.svg)](https://casuallymaintained.tech/)
[![Vibe Coded](https://img.shields.io/badge/vibe_coded-Claude-d97757?logo=anthropic&logoColor=white)](https://claude.com/claude-code)

Track your training sessions, monitor progress, and celebrate personal records.

## Features

- **Workout tracking** — sessions, exercises, sets, reps, weight, and RPE (1–10)
- **Fewer keystrokes** — Add Set suggests weights already used in the workout, each exercise's last weight, and common rep schemes
- **Personal records** — detected automatically, all-time and over a rolling month
- **Exercise library** and **per-exercise progress stats**
- **Multi-user** with authentication
- **Docker image**

## Quick Start

### Docker (recommended)

```bash
docker run -d \
  --name liftlog \
  -p 8080:8080 \
  -v liftlog_data:/data \
  ghcr.io/henry40408/liftlog:latest
```

Visit `http://localhost:8080` and create your account.

### From source

```bash
git clone https://github.com/henry40408/liftlog.git
cd liftlog
cargo build --release
./target/release/liftlog
```

## Configuration

All configuration is via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:liftlog.sqlite3?mode=rwc` | SQLite connection string |
| `LIFTLOG_BIND` | `127.0.0.1:8080` | Bind address. Loopback by default; the container image sets `0.0.0.0:8080`. |
| `LIFTLOG_TRUSTED_PROXY_HEADER` | (unset) | Header trusted to carry the client IP for the per-IP login limit: `x-forwarded-for` or `x-real-ip`. **Your proxy MUST overwrite it** (nginx: `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;` or `proxy_set_header X-Real-IP $remote_addr;`), otherwise clients can forge their IP. Unset, no header is read and every client behind a proxy shares one bucket. |
| `LIFTLOG_TRUSTED_PROXIES` | (empty) | Comma-separated IPs of proxies allowed to supply that header. Loopback is always trusted. The **rightmost** `X-Forwarded-For` hop is used. A proxy in a separate container must be listed here. |
| `LIFTLOG_COOKIE_SECURE` | `false` | Set `true` for HTTPS (including behind a TLS-terminating proxy); leave `false` on plain HTTP or the browser silently drops the cookie. `true` also renames the cookie to `__Host-session`, so flipping it logs everyone out once. |
| `LIFTLOG_HSTS_MAX_AGE` | `0` | `Strict-Transport-Security` `max-age` in seconds; `0`/unset/empty sends no header. |
| `LIFTLOG_HSTS_INCLUDE_SUBDOMAINS` | `false` | Add `includeSubDomains` to that header. |
| `RUST_LOG` | `error,liftlog=info` | Log filter |
| `LIFTLOG_LOG_FORMAT` | `full` | `full`, `compact`, `pretty`, or `json` (also `--log-format`) |

> **Migration note:** `BIND` and `LOG_FORMAT` were renamed to `LIFTLOG_BIND` and `LIFTLOG_LOG_FORMAT`. If an old name is still set, the server refuses to start and names the replacement.

### Reverse proxy

- **Forward `Host` exactly as the browser sent it, port included** (nginx: `proxy_set_header Host $http_host;`, not `$host`). The CSRF guard compares `Origin` against host *and port*, and for requests without `Sec-Fetch-Site` (Safari < 16.4, any plain-HTTP origin) that is the only check. Getting it wrong yields `403`s logged as `csrf.rejected` / `reason=origin_fallback`. Caddy and Traefik forward `Host` unchanged by default.
- **Prefer sending HSTS from the proxy** — liftlog doesn't terminate TLS. `LIFTLOG_HSTS_MAX_AGE` is an escape hatch; before enabling it make sure the whole domain (and subdomains, with `includeSubDomains`) serves HTTPS, since HSTS can only be waited out. There is no `preload` option. Set HSTS in one place only.

## Security

### Sessions

- Logout sends `Clear-Site-Data: "cache", "cookies", "storage"`. `"cookies"` covers the whole **registrable domain**, so sibling services (e.g. `wiki.example.com` next to `liftlog.example.com`) are logged out too. Ignored on plain HTTP.
- Promoting a user to admin logs that user out everywhere, so a token stolen before the promotion can't inherit admin rights.
- Changing your password signs out every other device **and** rotates your own token; you stay signed in with the new one.
- Promoting or deleting a user requires the acting admin to re-enter their own password on a confirmation page. This defends against a stolen cookie or an unlocked browser, which the CSRF guard can't. It shares the password-change rate limit.

### Login throttling

- **Per IP:** `POST /auth/login` allows 5 attempts per 60 seconds.
- **Per account** (keyed by the submitted username): 3 free failures, then each attempt is delayed 1s, 2s, 4s … up to 30s; forgotten after an hour of quiet, cleared by a correct password. Deliberately a delay, **not a lockout** — with no email, no password reset, and a single admin, a lockout would let anyone permanently lock the owner out. Unknown usernames accumulate the same penalty, so the delay reveals nothing.
- Failed logins cost the same for unknown and existing usernames (an Argon2 check against a dummy hash), so response time doesn't reveal which accounts exist.
- **Password change** (`POST /settings/password`) allows 5 attempts per 15 minutes, keyed by **user id** so a stolen session can't get a fresh budget from new IPs. A successful change refunds its attempt.

### Passwords

Passwords must be **12–128 characters** (counted as characters, not bytes; never truncated) **and score ≥ 3 of 4 on [zxcvbn](https://github.com/dropbox/zxcvbn)**, which runs offline and also rejects passwords built from the username. So `MyPassword12` is refused and `deadlift squats bench` accepted. Refusals show zxcvbn's feedback but not its crack-time estimate.

The 12-character floor is below NIST SP800-63B's 15 for non-MFA deployments on purpose: 12 plus zxcvbn rejects strictly more weak passwords than 15 alone (`123456789012345` is 15 characters). Both thresholds are constants in `src/models/user.rs`; changing them doesn't invalidate stored passwords. zxcvbn's dictionaries are English-centric, so passwords in other scripts rely mainly on the length floor.

### Headers

Every response carries `Content-Security-Policy: frame-ancestors 'none'`, `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, and `Referrer-Policy: strict-origin-when-cross-origin`, unconditionally. The first two block clickjacking, which `SameSite=Lax` and the CSRF guard don't; as a result **liftlog cannot be embedded in an iframe**, including `/shared/{token}`. The CSP only sets `frame-ancestors`; it doesn't restrict scripts or styles.

### Out of scope

- **MFA is not planned.** liftlog has no email, no password reset, and its first user is the sole admin; an admin who lost their authenticator and recovery codes could only get back in by editing the database. For a personal journal that risk outweighs the benefit. The residual risk is credential stuffing with a reused password — use a password manager (standard fields, correct `autocomplete`, 128 characters of anything).
- **No breach-corpus check (e.g. [Pwned Passwords](https://haveibeenpwned.com/Passwords)).** zxcvbn measures guessability, which stops password spraying; it can't know that a strong password was reused and leaked elsewhere. Checking that needs an outbound API call, which liftlog deliberately never makes. Same mitigation: a unique password per site.
- **Usernames are case-sensitive.** `henry` and `Henry` are distinct accounts, and a wrong-case username fails with the same generic `Invalid username or password`. A test pins this. If it ever changes, the per-account login backoff (keyed by the submitted username) must be normalised in the same change, or varying the case would bypass it.

## Audit Log

Security events are structured `tracing` events under the `liftlog::audit` target. Set `LIFTLOG_LOG_FORMAT=json` for one JSON event per line.

| Event | Level | Meaning |
|-------|-------|---------|
| `session.created` | info | Session created (`reason`: `login`, `setup`, `password_change_rotation`) |
| `session.renewed` | info | Sliding expiry extended a session |
| `session.destroyed` | info | One or more sessions deleted (`reason`: `logout`, `password_change`, `logout_others`, `role_change`, `admin_user_delete`) |
| `session.expired` | info | Expired session found on use (`reason`: `idle`, `absolute`) or retired by the hourly sweep (`reason`: `sweep`, `count` only) |
| `session.rejected` | debug | Unknown session token presented; `debug` so cookie-probing scanners don't drown other events |
| `auth.login.failed` | warn | Login rejected; carries `username` (≤256 chars) and `backoff_ms`. Identical for unknown users and wrong passwords. A password typed into the username field ends up here. |
| `auth.login.throttled` | warn | Login refused by the rate limiter |
| `auth.reauth.failed` | warn | Wrong password on a re-auth check; carries `user_id`, `actor_session_fp`, `action` (`password_change`, `promote_user`, `delete_user`) |
| `auth.reauth.throttled` | warn | Re-auth refused by the per-user limiter; same `action` |
| `csrf.rejected` | warn | State-changing request refused as cross-site; carries `method`, `origin` (≤256 chars), and `reason`: `sec_fetch_site` (browser reported cross-site, `same-site` included) or `origin_fallback` (no `Sec-Fetch-Site`, `Origin` didn't match host and port) |

A steady trickle of `origin_fallback` usually means the proxy isn't forwarding `Host` correctly — see [Reverse proxy](#reverse-proxy).

Request-scoped events carry `client_ip`, `user_agent` (≤256 chars), `path`, and `session_fp` — a salted SHA-256 of the token, never the token itself. The salt is per process, so `session_fp` only correlates within one run. Bulk deletes carry `actor_session_fp` and `count` instead.

## Docker Compose

```yaml
services:
  liftlog:
    image: ghcr.io/henry40408/liftlog:latest
    ports:
      - "8080:8080"
    volumes:
      - liftlog_data:/data
    restart: unless-stopped

volumes:
  liftlog_data:
```

Build locally with `docker build -t liftlog:latest .`

## Development

SQLite is bundled via rusqlite; only a Rust toolchain is needed.

```bash
cargo run
cargo nextest run
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

### E2E tests

`e2e/` ([cucumber](https://github.com/cucumber-rs/cucumber) + [thirtyfour](https://github.com/stevepryde/thirtyfour)) is a separate Cargo workspace with Gherkin features in `e2e/features/`. It needs a local Chrome or Chromium (`brew install --cask ungoogled-chromium` on macOS):

```bash
cargo build          # the suite runs target/debug/liftlog as-is, even if stale
cd e2e
cargo test --test e2e
```

## Tech Stack

Axum 0.8 · Tokio · SQLite (rusqlite + r2d2) · Askama · Argon2

## License

MIT
