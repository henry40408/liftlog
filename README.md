# LiftLog

[![CI](https://github.com/henry40408/liftlog/actions/workflows/ci.yml/badge.svg)](https://github.com/henry40408/liftlog/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/henry40408/liftlog/graph/badge.svg)](https://codecov.io/gh/henry40408/liftlog)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE.txt)
[![Rust](https://img.shields.io/badge/rust-1.92-blue.svg)](https://www.rust-lang.org/)
[![Docker](https://img.shields.io/badge/docker-ghcr.io-blue.svg)](https://ghcr.io/henry40408/liftlog)
[![Casual Maintenance Intended](https://casuallymaintained.tech/badge.svg)](https://casuallymaintained.tech/)

A self-hosted workout logging application built with Rust. Track your training sessions, monitor progress, and celebrate personal records.

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
  -p 3000:3000 \
  -v liftlog_data:/data \
  ghcr.io/henry40408/liftlog:latest
```

Visit `http://localhost:3000` and create your account.

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
| `HOST` | `127.0.0.1` | Server bind address |
| `PORT` | `3000` | HTTP server port |

## Docker

### Docker Compose

```yaml
services:
  liftlog:
    image: ghcr.io/henry40408/liftlog:latest
    ports:
      - "3000:3000"
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
cargo test
```

### Code Quality

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

## Tech Stack

- **Web Framework**: Axum 0.7
- **Async Runtime**: Tokio
- **Database**: SQLite (rusqlite + r2d2)
- **Templates**: Askama
- **Password Hashing**: Argon2

## License

MIT
