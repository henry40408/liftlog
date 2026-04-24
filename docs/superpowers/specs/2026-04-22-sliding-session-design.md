# Sliding Session + Logout Other Devices

Date: 2026-04-22
Status: Approved — awaiting implementation plan
Branch: `feat/sliding-session`

## Problem

Currently, `liftlog` issues a 7-day fixed-TTL session cookie at login and never extends it (`src/session.rs:4-12`, `src/repositories/session_repo.rs:18-37`). Users must re-authenticate exactly seven days after login regardless of activity. The goal is to keep active users logged in indefinitely while:

1. Keeping idle cookies short-lived so stolen/abandoned cookies die quickly.
2. Giving users a way to revoke sessions on other devices without changing their password.

## Decisions

| Decision | Value | Rationale |
|---|---|---|
| Auth model | Keep existing server-side opaque session tokens | Already DB-backed with per-request validation, so refresh-token / JWT machinery adds complexity without benefit. |
| Idle TTL | 7 days | Matches current absolute TTL; "7 days of inactivity" is a reasonable floor. |
| Absolute max lifetime | None | User preference: prioritise not interrupting active users. Server-side tokens can be revoked instantly if compromised. |
| Throttle for sliding writes | 1 hour | Upper-bounds DB write load to one UPDATE per session per hour regardless of traffic. Cookie and DB row stay within 1h of truth. |
| Sliding implementation | axum response middleware | Only `src/handlers/auth.rs` currently takes `CookieJar`; refreshing cookies per-handler would require touching every protected route. Middleware also lets us collapse the duplicate `find_valid` call in `OptionalAuthUser`. |
| Migration strategy | 12-step table rebuild | SQLite forbids `ALTER TABLE ADD COLUMN NOT NULL DEFAULT CURRENT_TIMESTAMP`. A table rebuild keeps `last_touched_at` NOT NULL without leaking nullable semantics into Rust. |
| Default logout | Unchanged — revokes only the current session | Matches user expectation from every mainstream web app. |
| New "logout other sessions" surface | Settings page with active-session list + button | User picked this over a naked button: lets users see what they're about to kill. |
| Session list fields | `created_at`, `last_touched_at`, `is_current` | User explicitly chose not to capture User-Agent or IP — no PII. |

## Data Model

Migration `010_rebuild_sessions_with_last_touched_at.sql`:

```sql
PRAGMA foreign_keys = OFF;

CREATE TABLE sessions_new (
    token TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    last_touched_at DATETIME NOT NULL
);

INSERT INTO sessions_new (token, user_id, created_at, expires_at, last_touched_at)
    SELECT token, user_id, created_at, expires_at, created_at
    FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_new RENAME TO sessions;

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

PRAGMA foreign_keys = ON;
```

Existing rows have `last_touched_at` backfilled to `created_at` — conservative: the first post-migration request from an active user will trigger a touch because `now - created_at > 1h` for any aged session. Tokens and expiries are preserved, so **no one is logged out by the migration**.

## Sliding Session Flow (Commit C1)

### Shape

```
request
  ↓
sliding_session_middleware  (validate + touch + stash ValidatedSession)
  ↓
AuthUser / OptionalAuthUser extractor  (reads Extension<ValidatedSession>, fetches user)
  ↓
handler
  ↓
sliding_session_middleware  (if touched, inject Set-Cookie with new max-age)
  ↓
response
```

### Contracts

`src/session.rs` gains:

```rust
pub const SESSION_IDLE_TTL: chrono::Duration = chrono::Duration::days(7);
pub const SESSION_TOUCH_THROTTLE: chrono::Duration = chrono::Duration::hours(1);
```

`SessionRepository` contract changes:

- `find_valid(token)` is **replaced** by `validate_and_touch(token) -> Option<Touched>` where
  ```rust
  pub struct Touched {
      pub user_id: String,
      pub new_expires_at: Option<DateTime<Utc>>, // Some(_) ⇔ row was UPDATEd this call
  }
  ```
  Behaviour: lookup → if `expires_at <= now` delete and return `None` → else if `now - last_touched_at > SESSION_TOUCH_THROTTLE` UPDATE `last_touched_at = now, expires_at = now + SESSION_IDLE_TTL` and return `Some(Touched { user_id, new_expires_at: Some(now + IDLE_TTL) })` → else `Some(Touched { user_id, new_expires_at: None })`.
- New: `list_for_user(user_id) -> Vec<SessionListRow>` returning non-expired rows ordered by `last_touched_at DESC`. Fields: `token`, `created_at`, `last_touched_at`.
- Retained: `create`, `delete`, `delete_all_for_user_except`, `cleanup_expired`.

`ValidatedSession` (new, request extension):

```rust
pub struct ValidatedSession {
    pub user_id: String,
    pub session_token: String,
    pub new_expires_at: Option<DateTime<Utc>>,
}
```

`src/middleware/auth.rs`:

- New `sliding_session_middleware` applied via `axum::middleware::from_fn_with_state` on protected routes. Extracts cookie token → calls `validate_and_touch` → inserts `ValidatedSession` into request extensions (or skips if invalid — downstream extractor will redirect) → runs next → if `new_expires_at.is_some()`, adds `Set-Cookie` header to the response with `Max-Age = SESSION_IDLE_TTL`.
- `AuthUser` and `OptionalAuthUser` extractors are **slimmed**: they read `Extension<ValidatedSession>` rather than invoking `SessionRepository` themselves. They still fetch the `User` row via `UserRepository`.

### Cookie

`create_session_cookie` already sets `Max-Age = 7 days`. The middleware re-issues the cookie on touch with the same semantics — no schema change on the cookie itself.

### Route wiring

The sliding middleware is attached to every route group that currently requires `AuthUser` / `OptionalAuthUser`. Login / setup / static routes stay unwrapped.

## Logout Other Devices + Settings UI (Commit C2)

### Endpoint

`POST /settings/logout-others` in `src/handlers/settings.rs` → takes `AuthUser` → `session_repo.delete_all_for_user_except(user_id, auth_user.session_token)` → renders `SettingsTemplate { success: Some("Logged out of all other devices"), ... }`. No redirect — same pattern as the existing change-password form. Lives alongside change-password because it renders the settings template; keeping it out of `auth.rs` avoids a cross-module template dependency.

### Settings template

`templates/settings/index.html` grows a new section below the change-password block:

```
Active sessions
───────────────
• (this device) — last used: 2 minutes ago · signed in: Mar 14
• last used: 3 hours ago · signed in: Mar 10
• last used: yesterday · signed in: Feb 28

[ Log out all other devices ]
```

The submit button triggers `onsubmit="return confirm('Log out of all other devices?')"` — no dedicated confirm page.

### Handler plumbing

`src/handlers/settings.rs::index` additionally calls `session_repo.list_for_user(auth_user.id).await` and passes the list plus `current_token = auth_user.session_token` into `SettingsTemplate`. Each row in the template compares `row.token == current_token` to render the "this device" marker. Tokens are not rendered in HTML.

## Testing

### C1 (sliding session)

`src/repositories/session_repo.rs` unit tests:
- `validate_and_touch` within throttle window: row unchanged, `new_expires_at` is `None`.
- `validate_and_touch` outside throttle window: `last_touched_at` and `expires_at` both advance; `new_expires_at` is `Some`.
- `validate_and_touch` on expired row: row deleted, returns `None`.
- `list_for_user` filters expired rows and orders by `last_touched_at DESC`.

`tests/auth_test.rs` integration tests:
- Active session survives past the original 7-day mark (advance clock ×2 within TTL, then check still valid).
- Idle session expires at the 7-day mark from last touch.
- Cookie `Max-Age` is re-issued on touched responses and absent on non-touched responses.

### C2 (logout others)

`tests/auth_test.rs`:
- Settings page lists sessions with "this device" marker on the correct row.
- `POST /auth/logout-others` deletes sibling sessions but not the current one; subsequent request from killed session redirects to `/auth/login`.
- Confirmation flow: form includes `onsubmit` confirm attribute (structural assertion on rendered HTML).

## Commit Plan

Single branch `feat/sliding-session`, two commits:

1. **C1 — `feat(auth): add sliding session with idle TTL`**
   Migration `010_…`, `SESSION_IDLE_TTL` / `SESSION_TOUCH_THROTTLE` constants, `validate_and_touch`, `sliding_session_middleware`, slimmed extractors, route wiring, tests.
2. **C2 — `feat(settings): add "log out all other devices"`**
   `list_for_user`, `logout_others` handler in `handlers/settings.rs`, `POST /settings/logout-others` route, settings template section, tests.

Tests must stay green between commits.

## Non-Goals

- **Refresh tokens / JWT.** Explicitly rejected above.
- **Absolute session max lifetime.** User chose to skip; can be added later without schema churn (just add a `created_at` comparison in `validate_and_touch`).
- **"Remember me" toggle with differentiated TTL.** Out of scope.
- **Device fingerprinting / User-Agent / IP capture.** User rejected on privacy grounds.
- **CSRF tokens.** The existing `POST /auth/logout` has no CSRF token either — adding CSRF is a separate, codebase-wide concern.
- **Background session cleanup job.** `cleanup_expired` is already invoked opportunistically on login (`src/handlers/auth.rs:90`).

## Open questions

None at write time.
