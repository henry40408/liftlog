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
| `BIND` | `0.0.0.0:8080` | HTTP server bind address (`host:port`) |
| `RUST_LOG` | `error,liftlog=info` | Log level filter |
| `LOG_FORMAT` | `full` | Log output format: `full`, `compact`, `pretty`, `json` (also settable via `--log-format`) |

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
