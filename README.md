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
| `LIFTLOG_HSTS_MAX_AGE` | `0` (disabled) | Seconds for the `Strict-Transport-Security` header's `max-age`. `0`, unset, or empty sends no header. |
| `LIFTLOG_HSTS_INCLUDE_SUBDOMAINS` | `false` | Whether the `Strict-Transport-Security` header, when `LIFTLOG_HSTS_MAX_AGE` is set, also carries `includeSubDomains`. |
| `RUST_LOG` | `error,liftlog=info` | Log level filter |
| `LIFTLOG_LOG_FORMAT` | `full` | Log output format: `full`, `compact`, `pretty`, `json` (also settable via `--log-format`) |

On logout, liftlog sends `Clear-Site-Data: "cache", "cookies", "storage"` so the browser drops more than just the session cookie. The `"cookies"` directive's scope is the whole **registrable domain**, not just this origin — if liftlog shares a domain with other services (e.g. `liftlog.example.com` alongside `wiki.example.com`), logging out of liftlog will also log the user out of those. Browsers ignore the header entirely on non-secure (plain HTTP) origins.

Promoting a user to admin logs that user out of every device. A privilege-level change requires reauthentication (OWASP Session Management Cheat Sheet, *Renew the Session ID After Any Privilege Level Change*), so a token stolen while the account was an ordinary user cannot silently inherit admin rights.

Promoting or deleting a user also requires the acting admin to re-enter **their own** password, on a confirmation page that spells out what is about to happen (OWASP Authentication Cheat Sheet, *Require Re-authentication for Sensitive Features*). The CSRF origin guard already blocks a cross-site *trigger* of those routes; what it cannot stop is someone who holds the admin's session cookie outright, or who has walked up to an unlocked browser. This re-check turns "has the cookie" into "knows the password" for the two actions that can hand out admin rights or destroy an account. It shares the password change's per-user rate limit, so an attacker cannot move their guessing from one route to the other for a fresh allowance.

Changing your password rotates your own session token, not just everyone else's. Every other device was already signed out; what the rotation adds is that the token in your *own* browser is replaced too, so a token captured before the change stops working after it — which matters precisely because rotating a password you believe is compromised is the case this is for. The replacement cookie comes back on the same response, so you stay signed in.

A failed login costs the same whether or not the username exists. The response wording is already generic, and an unknown username now spends an Argon2 verification against a throwaway hash so the two paths cannot be told apart by response time either — otherwise a single request would reveal whether an account exists, letting an attacker aim the login rate limit at real accounts only.

Repeated failed logins against the *same account* are slowed down, keyed by the submitted username: three failures are free, then each further attempt is held 1s, 2s, 4s … up to 30s, and the penalty is forgotten after an hour of quiet. A correct password clears it immediately, so mistyping your own password a few times costs you nothing lasting.

This is deliberately a delay and **not** an account lockout, which is what OWASP names first. The cheat sheet also warns that lockout is a denial-of-service primitive — anyone can lock anyone out — and suggests letting a forgotten-password flow rescue a locked account. liftlog has no such flow, no email, and its first user is its only administrator, so a hard lockout would let an unauthenticated attacker permanently lock the owner out of their own data with no recovery short of editing the database by hand. The delay collapses an attacker's guess rate just as effectively while leaving every legitimate login eventually possible.

It complements the per-IP limit rather than duplicating it: that one bounds how fast a single source can try, this one bounds how fast *one account* can be tried no matter how many sources are used — which is the shape of a password spray. The penalty accumulates for usernames that do not exist exactly as for real ones, so the wait cannot be used to ask whether an account exists.

Changing a password is throttled too, at 5 attempts per 15 minutes. `POST /settings/password` verifies the current password, which makes it liftlog's second place a password can be guessed at — reachable by anyone holding a stolen session cookie, and costing two Argon2 operations per request. Unlike the login throttle this one is keyed by **user id**, not client IP: the request is authenticated, so the account under attack is known exactly, and an IP key would let the same stolen session buy a fresh budget from every source address. A successful change hands its attempt back, so rotating your password repeatedly never locks you out.

Passwords must be 12 to 128 characters **and** score at least 3 of 4 on [zxcvbn](https://github.com/dropbox/zxcvbn). The maximum is there so the hash comparison has a bounded input (OWASP *Compare Password Hashes Using Safe Functions*); over-long passwords are rejected, never silently truncated. Both bounds count **characters, not bytes**, so a non-ASCII passphrase is measured the same way the error message describes it.

The strength check covers the *common* half of OWASP's *Block common and previously breached passwords* requirement — the *previously breached* half is deliberately not covered, for reasons set out under [Out of scope](#out-of-scope). zxcvbn ships the common-password and English-word dictionaries plus pattern detection (keyboard walks, l33t substitution, dates, repeats), and runs entirely offline — nothing about your password leaves the process, unlike a Pwned Passwords API lookup. It also receives your username, so a password built out of it is rejected. The upshot is that `MyPassword12` is refused while `deadlift squats bench` is accepted: the policy measures guessability, not whether you remembered to add a digit. When a password is refused, zxcvbn's own explanation of *why* is shown; its guess-count and crack-time estimates deliberately are not, since the cheat sheet warns against advertising an entropy figure as a guarantee of strength.

The 12-character floor is below NIST SP800-63B's 15 for deployments without MFA, and that is a deliberate pairing rather than an oversight: NIST's own advice is length *and* blocklist checks over composition rules, and 12-plus-zxcvbn rejects strictly more weak passwords than 15 alone would (`123456789012345` is 15 characters). Both the length floor and the score threshold are single constants in `src/models/user.rs` if you want a stricter bar. Changing either does not invalidate stored passwords — existing users keep working until they next set one.

Note that zxcvbn's dictionaries are English-centric, so a password in another script gets little signal from the strength check and is protected mainly by the length floor.

Every response carries `Content-Security-Policy: frame-ancestors 'none'`, `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff` and `Referrer-Policy: strict-origin-when-cross-origin`. These are unconditional — there is no setting to turn them off. The first two block clickjacking, which nothing else here covers: `SameSite=Lax` still sends the session cookie on a top-level iframe navigation, and the CSRF origin guard sees `Sec-Fetch-Site: same-origin` because the click really did come from the victim's browser. This means **liftlog cannot be embedded in an iframe**, including the public `/shared/{token}` page. The CSP carries only `frame-ancestors`; it is not a full content policy, so it does not restrict scripts or styles.

Prefer sending HSTS from your reverse proxy. liftlog does not terminate TLS and cannot tell whether a request really arrived over HTTPS; the layer that terminates TLS does. `LIFTLOG_HSTS_MAX_AGE` is an escape hatch for deployments that cannot set headers at the proxy. Before enabling it, make sure the whole domain — and, with `LIFTLOG_HSTS_INCLUDE_SUBDOMAINS`, every subdomain — serves working HTTPS: this declaration cannot be withdrawn from the server side, only waited out until `max-age` expires. There is deliberately no `preload` option; configure that on your proxy if you want it. Browsers ignore the header on plain-HTTP origins, so setting it there achieves nothing. If your proxy also sends HSTS, set it in only one place.

### Out of scope

**Multi-factor authentication is not implemented, and is not planned.** The OWASP Authentication Cheat Sheet calls MFA the single most effective defence against password attacks, so this is a deliberate decision rather than a gap waiting to be filled — please read the reasoning before filing it as a bug.

The blocker is account recovery, not the TOTP implementation. liftlog has no email, no password-reset flow, and no second channel of any kind to reach a user through. The first account created is the sole administrator. If that administrator enrolled in MFA and later lost both their authenticator and their recovery codes, nobody could restore their access — there is no support desk, and the only way back in would be editing the SQLite file by hand. For a personal, self-hosted workout journal, a realistic risk of permanently locking the owner out of their own data outweighs the attack it would prevent.

What that leaves as residual risk is **credential stuffing**: a password reused from a site that was breached elsewhere. The strength policy above blocks *common* passwords, but a reused password can be strong and still be in someone's breach corpus — that is the case MFA would have covered and nothing here does. The mitigation is a unique password per site, which a password manager makes free. liftlog is deliberately friendly to them: standard form fields, correct `autocomplete` attributes, a 128-character ceiling, and every character allowed.

**Checking passwords against a breach corpus such as [Pwned Passwords](https://haveibeenpwned.com/Passwords) is also out of scope**, and it is worth being precise about why the strength check above does not already cover it — it looks like it should.

zxcvbn and a breach lookup answer different questions. zxcvbn asks *"is this password guessable?"* — would an attacker's model generate it early. A breach lookup asks *"has this exact string ever appeared in a dump?"* — a fact about history, not a property of the string. A password can be maximally unguessable and still be in the corpus, because its owner reused it and some other site was breached. No guessability model can know that.

Put in terms of the attacks: zxcvbn is a direct hit against **password spraying**, which works through frequency-ordered guesses. It does nothing against **credential stuffing**, which replays exact `username:password` pairs lifted from a breach. Scale makes the point too — zxcvbn ships 30,000 passwords; Pwned Passwords holds several hundred million hashes. That is not a dictionary anything embeds.

Closing it properly therefore needs an outbound API call at password-set time. The k-anonymity protocol means the password itself never leaves (only the first five characters of its SHA-1, matched against the returned suffixes locally), but it still turns an application that contacts nothing into one that contacts something, and it needs an answer for what to do when the service is unreachable. For a self-hosted personal journal that trade is not obviously worth making, so it is not made. The residual risk is the credential-stuffing paragraph above, and the mitigation is the same: a unique password per site.

**Usernames are case-sensitive, and making them case-insensitive is out of scope.** A username is treated as an exact identifier: `henry` and `Henry` are different accounts and can both exist. Two consequences are worth knowing rather than discovering:

- Typing your username in the wrong case fails with the same generic `Invalid username or password` as a wrong password would. That wording is deliberate — a more specific message would tell an attacker which usernames exist — but it does mean a case slip looks identical to a forgotten password.
- An administrator can create `Admin` alongside `admin`. In a deployment with more than one person, decide your own convention; nothing enforces one.

This is enforced by a test, so it cannot drift by accident. If it is ever revisited, note that the per-account login backoff is keyed by the **submitted** username: any change that makes lookup case-insensitive has to normalise that key — and anything else keyed by username — in the same change, or an attacker can vary the case to get a fresh backoff counter per spelling and bypass the throttle entirely.

## Audit Log

Session lifecycle events (OWASP Session Management Cheat Sheet, *Logging Sessions Life Cycle*) are logged as structured `tracing` events under the `liftlog::audit` target, so they can be filtered out of general application logs and shipped to a log collector:

| Event | Level | Meaning |
|-------|-------|---------|
| `session.created` | info | Login or first-user setup created a session (`reason`: `login` or `setup`) |
| `session.renewed` | info | The sliding-expiry touch extended a session's lifetime |
| `session.destroyed` | info | A session (or, for a bulk delete, a batch of sessions) was deleted — logout, password change, "log out other devices", an admin promoting the user to admin, or an admin deleting the user (`reason` says which) |
| `session.expired` | info | A session was found dead on use and lazily deleted (`reason`: `idle` or `absolute`), or a batch of abandoned sessions was retired by the hourly background sweep (`reason`: `sweep`, which carries only a `count` and no request fields) |
| `session.rejected` | debug | An unrecognised session token was presented |

Authentication failures are logged alongside them (OWASP Authentication Cheat Sheet, *Logging and Monitoring*: all password failures and all lockouts must be logged and reviewed). These are the events to alert on — a burst of them is what a brute-force or credential-stuffing run looks like:

| Event | Level | Meaning |
|-------|-------|---------|
| `auth.login.failed` | warn | A login was rejected. Carries the attempted `username` (truncated to 256 chars) so you can see which account is being targeted, and `backoff_ms` — how long the per-account delay held that attempt, which climbs as an attack continues |
| `auth.login.throttled` | warn | A login was refused by the rate limiter before any credential was checked |
| `auth.reauth.failed` | warn | A route that re-checks the password before acting was given the wrong one. Carries `user_id`, `actor_session_fp` and `action` (`password_change`, `promote_user`, `delete_user`) |
| `auth.reauth.throttled` | warn | Such a re-check was refused by the per-user rate limiter. Same `action` field |

`auth.login.failed` is emitted identically for an unknown username and a wrong password — same event, same wording, same fields. Distinguishing them would rebuild in the log the user-enumeration oracle that the constant-cost login path exists to remove. Note the trade-off inherent in recording the attempted username at all: a user who types their password into the username field puts it in the log, the same way `sshd` does.

Every request-scoped event carries `client_ip`, `user_agent` (truncated to 256 chars), and `path`, plus a `session_fp` field — a salted SHA-256 fingerprint of the session token, never the raw token itself. The salt is generated fresh at process startup and is never logged, so `session_fp` values let you correlate events for the same session **within one process's lifetime**, but they do NOT correlate across restarts. Bulk-delete events carry `actor_session_fp` (the session that performed the action) and `count` instead of a single `session_fp`, since there's no one session to name. The sweep event is an exception: it has no request context and carries only `count`.

`session.rejected` is logged at `debug`, not `info`, because liftlog is typically internet-facing and scanners probing random cookie values would otherwise drown the genuinely useful events; set `RUST_LOG` to include `debug` to see them.

Set `LIFTLOG_LOG_FORMAT=json` to emit these (and all other logs) as JSON, one event per line, ready for ingestion by a log collector.

> **Migration note:** `BIND` and `LOG_FORMAT` were renamed to `LIFTLOG_BIND` and `LIFTLOG_LOG_FORMAT`. If either old name is still set in the environment, the server **refuses to start** and names the replacement, so a stale value can't be silently ignored.

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
