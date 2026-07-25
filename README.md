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

- **Workout Tracking** - Log training sessions with exercises, sets, reps, and weight
- **RPE Support** - Record Rate of Perceived Exertion (1-10) for each set
- **Personal Records** - Automatic PR detection and tracking
- **Exercise Library** - Manage your custom exercise database
- **Statistics** - View workout history and progress per exercise
- **Multi-User** - Support for multiple users with authentication
- **Docker Ready** - Container image for easy deployment

## Quick Start

### Using Docker (Recommended)

```bash
docker run -d \
  --name liftlog \
  -p 8080:8080 \
  -v liftlog_data:/data \
  ghcr.io/henry40408/liftlog:latest
```

Visit `http://localhost:8080` and create your account.

### Building from Source

```bash
# Clone repository
git clone https://github.com/henry40408/liftlog.git
cd liftlog

# Build release binary
cargo build --release

# Run server
./target/release/liftlog
```

## Configuration

All configuration is done via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:liftlog.sqlite3?mode=rwc` | SQLite database connection string |
| `LIFTLOG_BIND` | `127.0.0.1:8080` | HTTP server bind address (`host:port`). Defaults to loopback so a bare-metal run is not exposed on all interfaces without opting in; the container image sets `0.0.0.0:8080` so a reverse proxy can reach it. |
| `LIFTLOG_TRUSTED_PROXY_HEADER` | (unset / `none`) | Which proxy-supplied header, if any, liftlog trusts to carry the real client IP for per-IP login rate limiting (`POST /auth/login` allows 5 attempts per 60 seconds): `x-forwarded-for` or `x-real-ip`. This is a deliberate operator choice, not inferred from which headers happen to be present — the mere presence of an `X-Forwarded-For` line is not proof a trusted proxy wrote it. **Whichever header you select, your reverse proxy MUST overwrite it (or strip any client-supplied copy)**, e.g. nginx: `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;` (appends the peer; liftlog reads the rightmost hop) or `proxy_set_header X-Real-IP $remote_addr;`. If the proxy merely passes a client-supplied header through untouched, an attacker can forge their source IP and the login rate limit is bypassable. Left unset, no header is ever read and all clients behind a proxy share one login rate-limit bucket — a real limitation, but a safe default. |
| `LIFTLOG_TRUSTED_PROXIES` | (empty) | Comma-separated bare IPs of reverse proxies allowed to supply the header selected by `LIFTLOG_TRUSTED_PROXY_HEADER`; only consulted when that variable is set. The **rightmost** hop in `X-Forwarded-For` is used, since that's the one the trusted proxy itself appended. Loopback peers are always trusted regardless of this setting, so it's unnecessary when the proxy connects from loopback. If a reverse proxy runs in a separate container, its IP must be listed here, or every client will appear to share a single loopback peer address and thus one shared rate-limit bucket. |
| `LIFTLOG_COOKIE_SECURE` | `false` | Whether the session cookie carries the `Secure` attribute. Set `true` for HTTPS deployments, including behind a TLS-terminating reverse proxy. Leave `false` for plain-HTTP LAN deployments — otherwise the browser silently drops the cookie and login becomes impossible, with no error message. Setting it `true` also renames the cookie to `__Host-session` (the browser then enforces `Secure` + `Path=/` + no `Domain` at the protocol level), so flipping this setting invalidates existing logins once. |
| `RUST_LOG` | `error,liftlog=info` | Log level filter |
| `LIFTLOG_LOG_FORMAT` | `full` | Log output format: `full`, `compact`, `pretty`, `json` (also settable via `--log-format`) |

> **Migration note:** `BIND` and `LOG_FORMAT` were renamed to `LIFTLOG_BIND` and `LIFTLOG_LOG_FORMAT`. If either old name is still set in the environment, the server **refuses to start** and names the replacement, so a stale value can't be silently ignored.

On logout, liftlog sends `Clear-Site-Data: "cache", "cookies", "storage"` so the browser drops more than just the session cookie. The `"cookies"` directive's scope is the whole **registrable domain**, not just this origin — if liftlog shares a domain with other services (e.g. `liftlog.example.com` alongside `wiki.example.com`), logging out of liftlog will also log the user out of those. Browsers ignore the header entirely on non-secure (plain HTTP) origins.

## Audit Log

Session lifecycle events (OWASP Session Management Cheat Sheet, *Logging Sessions Life Cycle*) are logged as structured `tracing` events under the `liftlog::audit` target, so they can be filtered out of general application logs and shipped to a log collector:

| Event | Level | Meaning |
|-------|-------|---------|
| `session.created` | info | Login or first-user setup created a session (`reason`: `login` or `setup`) |
| `session.renewed` | info | The sliding-expiry touch extended a session's lifetime |
| `session.destroyed` | info | A session (or, for a bulk delete, a batch of sessions) was deleted — logout, password change, "log out other devices", or an admin deleting the user (`reason` says which) |
| `session.expired` | info | A session was found dead on use and lazily deleted (`reason`: `idle` or `absolute`) |
| `session.rejected` | debug | An unrecognised session token was presented |

Every event carries `client_ip`, `user_agent` (truncated to 256 chars), and `path`, plus a `session_fp` field — a salted SHA-256 fingerprint of the session token, never the raw token itself. The salt is generated fresh at process startup and is never logged, so `session_fp` values let you correlate events for the same session **within one process's lifetime**, but they do NOT correlate across restarts. Bulk-delete events carry `actor_session_fp` (the session that performed the action) and `count` instead of a single `session_fp`, since there's no one session to name.

`session.rejected` is logged at `debug`, not `info`, because liftlog is typically internet-facing and scanners probing random cookie values would otherwise drown the genuinely useful events; set `RUST_LOG` to include `debug` to see them.

Set `LOG_FORMAT=json` to emit these (and all other logs) as JSON, one event per line, ready for ingestion by a log collector.

## Docker

### Docker Compose

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

### Building Docker Image

```bash
docker build -t liftlog:latest .
```

## Development

### Prerequisites

- Rust (stable)
- SQLite (bundled via rusqlite)

### Running Locally

```bash
cargo run
```

### Running Tests

```bash
cargo nextest run
```

### UI BDD Tests

End-to-end tests live in `tests/e2e/` (Playwright + [playwright-bdd](https://github.com/vitalets/playwright-bdd)). They lock in user-facing behavior so UI redesigns can't silently change it. Features are described in Gherkin (`tests/e2e/features/`) and step bindings are plain JS (`tests/e2e/steps/`).

First-time setup:

```bash
cd tests/e2e
npm install
npm run install-browsers   # downloads Chromium
```

Run the suite (boots a fresh sqlite + Rust server per run):

```bash
cd tests/e2e
npm test                   # headless
npm run test:headed        # watch the browser
npm run test:ui            # interactive Playwright UI
npm run report             # open last HTML report
```

### Code Quality

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

## Tech Stack

- **Web Framework**: Axum 0.8
- **Async Runtime**: Tokio
- **Database**: SQLite (rusqlite + r2d2)
- **Templates**: Askama
- **Password Hashing**: Argon2

## License

MIT
