# Sliding Session + Logout Other Devices — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep active users logged in via a 7-day idle TTL that slides on each request, and add a "Log out all other devices" button on the settings page — without changing the default single-device logout behavior.

**Architecture:** Keep the existing server-side opaque session token model. Add a `last_touched_at` column to `sessions`. Introduce an Axum response middleware that (a) validates the session cookie and stashes a `ValidatedSession` into request extensions, and (b) re-issues the session cookie with a fresh `Max-Age` on touched responses. Slim `AuthUser` / `OptionalAuthUser` extractors to read `ValidatedSession` from extensions. Add a `POST /settings/logout-others` endpoint that reuses the existing `delete_all_for_user_except` repo method, rendered via the settings template with a success flash.

**Tech Stack:** Rust (edition follows workspace), Axum 0.8, axum-extra (CookieJar), rusqlite + r2d2, chrono, Askama templates, tokio, cargo-nextest.

**Working directory assumption:** every command below is run from `/home/nixos/Develop/claude/liftlog` (the repo root).

**Branch:** already on `feat/sliding-session`. The spec `docs/superpowers/specs/2026-04-22-sliding-session-design.md` is committed (`1e8cd2c`).

---

## File Structure

Files created:

- `migrations/010_rebuild_sessions_with_last_touched_at.sql` — 12-step table rebuild adding `last_touched_at DATETIME NOT NULL`.

Files modified:

- `src/migrations.rs` — register migration 010 in the `MIGRATIONS` array.
- `src/session.rs` — add TTL/throttle constants (seconds), make `create_session_cookie` drive its `Max-Age` from the idle-TTL constant.
- `src/repositories/session_repo.rs` — add `ValidateAndTouchOutcome` struct, replace `find_valid` with `validate_and_touch`, add `list_for_user`, rename/update existing tests. `create` populates `last_touched_at` on insert.
- `src/middleware/auth.rs` — add `ValidatedSession` struct and `sliding_session_middleware` (`pub async fn`), slim `AuthUser` / `OptionalAuthUser::from_request_parts` to read the extension.
- `src/routes.rs` — register `sliding_session_middleware` as a global `from_fn_with_state` layer and add the new `POST /settings/logout-others` route.
- `src/handlers/settings.rs` — `SettingsTemplate` gains `sessions` + `current_token` fields, `index` fetches sessions, new `logout_others` handler.
- `templates/settings/index.html` — new "Active Sessions" section + logout-others form with JS confirm.
- `tests/auth_test.rs` — integration tests for sliding behavior and logout-others.
- `tests/common/mod.rs` — no change expected.

Files not touched: `src/handlers/auth.rs` (login/logout flow unchanged), other handlers (auth extractors keep the same public shape, so dependents do not need to change).

---

## Phase C1 — Sliding Session (Commit 1)

### Task 1: Add migration 010 (table rebuild)

**Files:**
- Create: `migrations/010_rebuild_sessions_with_last_touched_at.sql`
- Modify: `src/migrations.rs`

**Why TDD doesn't apply here yet:** this is a schema-only change, and existing `session_repo` tests will assert we haven't broken any existing column or behavior. We will exercise the new column in Task 3.

- [ ] **Step 1.1: Create the migration file**

Create `migrations/010_rebuild_sessions_with_last_touched_at.sql` with:

```sql
-- Rebuild sessions table to add last_touched_at NOT NULL.
-- SQLite does not permit ALTER TABLE ADD COLUMN NOT NULL DEFAULT CURRENT_TIMESTAMP,
-- so we use the canonical 12-step rebuild. No tables reference sessions, so
-- turning foreign keys off is defensive only.
PRAGMA foreign_keys = OFF;

CREATE TABLE sessions_new (
    token TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    last_touched_at DATETIME NOT NULL
);

INSERT INTO sessions_new (token, user_id, created_at, expires_at, last_touched_at)
    SELECT token, user_id, created_at, expires_at, created_at FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_new RENAME TO sessions;

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

PRAGMA foreign_keys = ON;
```

- [ ] **Step 1.2: Register the migration**

Modify `src/migrations.rs` by appending an entry to the `MIGRATIONS` array (right after the `009_add_workout_share_token.sql` entry, ~line 37):

```rust
    (
        "010_rebuild_sessions_with_last_touched_at.sql",
        include_str!("../migrations/010_rebuild_sessions_with_last_touched_at.sql"),
    ),
```

- [ ] **Step 1.3: Verify build and existing tests still compile**

Run: `pwd && cargo check`
Expected: completes without errors (no code references the new column yet).

- [ ] **Step 1.4: Run existing session tests to confirm no behavioral regression**

Run: `pwd && cargo nextest run session`
Expected: all existing session tests pass — the extra column is additive and doesn't affect any current query.

- [ ] **Step 1.5: Stage (do NOT commit yet — bundled into Commit C1 at Task 8)**

Run:
```bash
git add migrations/010_rebuild_sessions_with_last_touched_at.sql src/migrations.rs
```

---

### Task 2: TTL/throttle constants in `session.rs`

**Files:**
- Modify: `src/session.rs`

**Rationale:** single source of truth for idle TTL and throttle interval. Declared in seconds as `i64` so both `chrono::Duration::seconds` and `time::Duration::seconds` can consume them in a `const` context.

- [ ] **Step 2.1: Add constants and thread them through `create_session_cookie`**

Replace `src/session.rs` entirely with:

```rust
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;

pub const SESSION_COOKIE_NAME: &str = "session";

/// How long a session survives without activity. A request within this
/// window (and outside the touch throttle) slides the expiry forward.
pub const SESSION_IDLE_TTL_SECS: i64 = 60 * 60 * 24 * 7; // 7 days

/// Minimum gap between two consecutive `last_touched_at` writes for the
/// same session. Keeps write load to at most one UPDATE per session per hour.
pub const SESSION_TOUCH_THROTTLE_SECS: i64 = 60 * 60; // 1 hour

pub fn create_session_cookie(token: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::seconds(SESSION_IDLE_TTL_SECS))
        .build()
}

pub fn get_session_token(jar: &CookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
}

pub fn remove_session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}
```

- [ ] **Step 2.2: Build**

Run: `pwd && cargo check`
Expected: no errors; no call-site changes needed.

- [ ] **Step 2.3: Stage**

Run: `git add src/session.rs`

---

### Task 3: Replace `find_valid` with `validate_and_touch` (TDD)

**Files:**
- Modify: `src/repositories/session_repo.rs`
- Test: same file's `#[cfg(test)] mod tests`

**Contract:**

```rust
pub struct ValidateAndTouchOutcome {
    pub user_id: String,
    /// Some(new_expires) iff this call advanced last_touched_at and expires_at.
    /// None means the throttle window absorbed the touch.
    pub new_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

Behavior:
- Row missing → `Ok(None)`.
- Row present and `expires_at <= now` → DELETE and `Ok(None)`.
- Row present, valid, and `now - last_touched_at > SESSION_TOUCH_THROTTLE_SECS` → UPDATE `last_touched_at = now`, `expires_at = now + SESSION_IDLE_TTL_SECS`, return `Ok(Some(ValidateAndTouchOutcome { user_id, new_expires_at: Some(new_expires) }))`.
- Row present, valid, within throttle → return `Ok(Some(ValidateAndTouchOutcome { user_id, new_expires_at: None }))`.

We also update `create` to populate `last_touched_at` at insert time.

- [ ] **Step 3.1: Write failing tests for `validate_and_touch`**

Replace the existing `test_create_and_find_valid`, `test_find_valid_nonexistent`, `test_find_valid_expired` tests and add the new touch-specific tests. Inside the `#[cfg(test)] mod tests` block of `src/repositories/session_repo.rs`, the full relevant test block becomes:

```rust
    #[tokio::test]
    async fn test_create_and_validate_and_touch_within_window() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token = repo.create(&user_id).await.unwrap();
        assert!(!token.is_empty());

        // Fresh session: last_touched_at is "now" so we are inside the throttle window.
        let outcome = repo.validate_and_touch(&token).await.unwrap().unwrap();
        assert_eq!(outcome.user_id, user_id);
        assert!(
            outcome.new_expires_at.is_none(),
            "touch should be absorbed by throttle window"
        );
    }

    #[tokio::test]
    async fn test_validate_and_touch_nonexistent() {
        let pool = setup_test_db();
        let repo = SessionRepository::new(pool);

        let found = repo.validate_and_touch("nonexistent-token").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_validate_and_touch_expired_deletes_row() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // Move expires_at into the past.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        assert!(outcome.is_none());

        // Row is gone.
        {
            let conn = pool.get().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE token = ?",
                    [&token],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn test_validate_and_touch_outside_window_slides_expiry() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // Simulate an old session: last_touched_at 2 hours ago (> 1h throttle),
        // expires_at still in the future.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET last_touched_at = datetime('now', '-2 hours'), \
                 expires_at = datetime('now', '+1 day') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let before_expires: chrono::DateTime<chrono::Utc> = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT expires_at FROM sessions WHERE token = ?",
                [&token],
                |row| row.get(0),
            )
            .unwrap()
        };

        let outcome = repo.validate_and_touch(&token).await.unwrap().unwrap();
        assert_eq!(outcome.user_id, user_id);
        let new_expires = outcome
            .new_expires_at
            .expect("touch should advance expiry outside throttle window");
        assert!(new_expires > before_expires);

        // last_touched_at was refreshed.
        let conn = pool.get().unwrap();
        let last_touched: chrono::DateTime<chrono::Utc> = conn
            .query_row(
                "SELECT last_touched_at FROM sessions WHERE token = ?",
                [&token],
                |row| row.get(0),
            )
            .unwrap();
        let age = chrono::Utc::now() - last_touched;
        assert!(age.num_seconds().abs() < 5, "last_touched_at should be ~now");
    }
```

Keep the existing `test_delete`, `test_delete_all_for_user_except`, `test_cleanup_expired` tests; internally change their calls from `repo.find_valid(...)` to `repo.validate_and_touch(...).map(...).is_some()` or equivalent, for example:

```rust
    #[tokio::test]
    async fn test_delete() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token = repo.create(&user_id).await.unwrap();
        repo.delete(&token).await.unwrap();

        let found = repo.validate_and_touch(&token).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_all_for_user_except() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token1 = repo.create(&user_id).await.unwrap();
        let token2 = repo.create(&user_id).await.unwrap();
        let token3 = repo.create(&user_id).await.unwrap();

        repo.delete_all_for_user_except(&user_id, &token2)
            .await
            .unwrap();

        assert!(repo.validate_and_touch(&token1).await.unwrap().is_none());
        assert!(repo.validate_and_touch(&token2).await.unwrap().is_some());
        assert!(repo.validate_and_touch(&token3).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token_valid = repo.create(&user_id).await.unwrap();
        let token_expired = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token_expired],
            )
            .unwrap();
        }

        repo.cleanup_expired().await.unwrap();

        assert!(repo.validate_and_touch(&token_valid).await.unwrap().is_some());

        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE token = ?",
                [&token_expired],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
```

- [ ] **Step 3.2: Run tests to verify they fail**

Run: `pwd && cargo nextest run -E 'test(session_repo)'`
Expected: compile errors or test failures citing missing `validate_and_touch` / `ValidateAndTouchOutcome`.

- [ ] **Step 3.3: Implement `validate_and_touch` and `create` update**

Replace the `create` and `find_valid` methods in `src/repositories/session_repo.rs`. The full relevant block (replacing lines that currently implement `create` + `find_valid`) is:

```rust
/// Returned by [`SessionRepository::validate_and_touch`].
pub struct ValidateAndTouchOutcome {
    pub user_id: String,
    /// `Some(new_expires)` iff this call wrote a new `last_touched_at` /
    /// `expires_at`. `None` means the call landed inside the throttle window.
    pub new_expires_at: Option<chrono::DateTime<Utc>>,
}

impl SessionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Create a new session for a user. Returns the session token.
    pub async fn create(&self, user_id: &str) -> Result<String> {
        let pool = self.pool.clone();
        let token = Uuid::new_v4().to_string();
        let user_id = user_id.to_string();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(crate::session::SESSION_IDLE_TTL_SECS);
        let token_clone = token.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) \
                 VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![token_clone, user_id, now, expires_at, now],
            )?;
            Ok(token_clone)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// Validate the session for a given token and, if the throttle window has
    /// elapsed, slide both `expires_at` and `last_touched_at` forward.
    /// Expired rows are lazily deleted.
    pub async fn validate_and_touch(
        &self,
        token: &str,
    ) -> Result<Option<ValidateAndTouchOutcome>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let now = Utc::now();
        let idle_ttl = chrono::Duration::seconds(crate::session::SESSION_IDLE_TTL_SECS);
        let throttle = chrono::Duration::seconds(crate::session::SESSION_TOUCH_THROTTLE_SECS);

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;

            let row: Option<(String, chrono::DateTime<Utc>, chrono::DateTime<Utc>)> = conn
                .query_row(
                    "SELECT user_id, expires_at, last_touched_at \
                     FROM sessions WHERE token = ?",
                    [&token],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;

            let Some((user_id, expires_at, last_touched_at)) = row else {
                return Ok::<_, AppError>(None);
            };

            if expires_at <= now {
                conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
                return Ok(None);
            }

            if now - last_touched_at > throttle {
                let new_expires = now + idle_ttl;
                conn.execute(
                    "UPDATE sessions SET last_touched_at = ?, expires_at = ? WHERE token = ?",
                    rusqlite::params![now, new_expires, token],
                )?;
                return Ok(Some(ValidateAndTouchOutcome {
                    user_id,
                    new_expires_at: Some(new_expires),
                }));
            }

            Ok(Some(ValidateAndTouchOutcome {
                user_id,
                new_expires_at: None,
            }))
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }
```

Keep `delete`, `delete_all_for_user_except`, `cleanup_expired` unchanged. Remove the old `find_valid` method entirely.

- [ ] **Step 3.4: Run tests to verify they pass**

Run: `pwd && cargo nextest run -E 'test(session_repo)'`
Expected: all session_repo tests pass.

- [ ] **Step 3.5: Build the rest of the crate (to expose any remaining `find_valid` callers)**

Run: `pwd && cargo check`
Expected: failures in `src/middleware/auth.rs` saying `find_valid` doesn't exist. These will be fixed in Tasks 4–5. We keep staging but do not commit until C1 is fully green.

- [ ] **Step 3.6: Stage**

Run: `git add src/repositories/session_repo.rs`

---

### Task 4: Introduce `ValidatedSession` + sliding middleware

**Files:**
- Modify: `src/middleware/auth.rs`

**Rationale:** middleware centralises the DB call, avoids the double-lookup `OptionalAuthUser` currently pays, and gives us a hook point to re-issue the cookie.

- [ ] **Step 4.1: Add `ValidatedSession` and `sliding_session_middleware`**

Edit `src/middleware/auth.rs`. At the top, after the existing `use` block, add:

```rust
use axum::extract::{Request, State};
use axum::middleware::Next;

use crate::repositories::SessionRepository;
use crate::session::{create_session_cookie, get_session_token};
```

Then add the `ValidatedSession` struct (below the existing `AuthUser` impl block, before `pub struct AuthRedirect`):

```rust
/// Produced by `sliding_session_middleware` for every request that arrives
/// with a valid session cookie. Extractors downstream read this from
/// request extensions instead of re-hitting the database.
#[derive(Clone, Debug)]
pub struct ValidatedSession {
    pub user_id: String,
    pub session_token: String,
}

/// Axum middleware that validates the session cookie, slides its expiry
/// when the touch throttle has elapsed, and (on touch) re-issues the
/// cookie with a fresh `Max-Age`. Applied globally; requests without a
/// cookie pass through untouched.
pub async fn sliding_session_middleware(
    State(session_repo): State<SessionRepository>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let token = get_session_token(&jar);
    let mut should_refresh_cookie: Option<String> = None;

    if let Some(tok) = token.as_deref() {
        match session_repo.validate_and_touch(tok).await {
            Ok(Some(outcome)) => {
                request.extensions_mut().insert(ValidatedSession {
                    user_id: outcome.user_id,
                    session_token: tok.to_string(),
                });
                if outcome.new_expires_at.is_some() {
                    should_refresh_cookie = Some(tok.to_string());
                }
            }
            Ok(None) | Err(_) => {
                // Invalid / expired token: do not insert ValidatedSession. The
                // downstream extractor (AuthUser) will redirect to /auth/login.
            }
        }
    }

    let mut response = next.run(request).await;

    if let Some(tok) = should_refresh_cookie {
        let cookie = create_session_cookie(&tok);
        if let Ok(header_value) = cookie.to_string().parse() {
            response
                .headers_mut()
                .append(axum::http::header::SET_COOKIE, header_value);
        }
    }

    response
}
```

- [ ] **Step 4.2: Build**

Run: `pwd && cargo check`
Expected: the middleware compiles. Errors may still exist elsewhere (`AuthUser` still references removed `find_valid`) — fix in Task 5.

- [ ] **Step 4.3: Stage**

Run: `git add src/middleware/auth.rs`

---

### Task 5: Slim `AuthUser` and `OptionalAuthUser` extractors

**Files:**
- Modify: `src/middleware/auth.rs`

- [ ] **Step 5.1: Rewrite both extractors to read `ValidatedSession` from extensions**

Replace the `AuthUser::from_request_parts` impl (`src/middleware/auth.rs:27-68`) and the `OptionalAuthUser::from_request_parts` impl (`src/middleware/auth.rs:81-123`) with:

```rust
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let validated = parts
            .extensions
            .get::<ValidatedSession>()
            .cloned()
            .ok_or(AuthRedirect)?;

        let Extension(user_repo) = Extension::<UserRepository>::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user = user_repo
            .find_by_id(&validated.user_id)
            .await
            .map_err(|_| AuthRedirect)?
            .ok_or(AuthRedirect)?;

        Ok(AuthUser {
            id: user.id,
            username: user.username,
            role: user.role,
            session_token: validated.session_token,
        })
    }
}

impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Some(validated) = parts.extensions.get::<ValidatedSession>().cloned() else {
            return Ok(OptionalAuthUser(None));
        };

        let Extension(user_repo) = Extension::<UserRepository>::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Session error"))?;

        let user = match user_repo.find_by_id(&validated.user_id).await {
            Ok(Some(u)) => u,
            _ => return Ok(OptionalAuthUser(None)),
        };

        Ok(OptionalAuthUser(Some(AuthUser {
            id: user.id,
            username: user.username,
            role: user.role,
            session_token: validated.session_token,
        })))
    }
}
```

After this change, the `SessionRepository`-related imports that were used by the old extractors (`crate::repositories::{SessionRepository, UserRepository}` → keep `UserRepository`) are no longer needed in the extractor bodies. Trim them if rustc warns.

- [ ] **Step 5.2: Build**

Run: `pwd && cargo check`
Expected: clean build (modulo formatting warnings). If anything references `find_valid`, it will surface here.

- [ ] **Step 5.3: Stage**

Run: `git add src/middleware/auth.rs`

---

### Task 6: Wire middleware into the router

**Files:**
- Modify: `src/routes.rs`

- [ ] **Step 6.1: Attach `sliding_session_middleware` as a global layer**

Edit `src/routes.rs`. Add to the imports:

```rust
use axum::middleware::from_fn_with_state;

use crate::middleware::sliding_session_middleware;
```

Modify the tail of `create_router` so the `session_repo` Extension and the middleware share state. Replace the current block (`src/routes.rs:86-88`):

```rust
        // Session + User repos via Extension layer for auth extractors
        .layer(Extension(session_repo))
        .layer(Extension(user_repo))
```

with:

```rust
        // Sliding session: validate cookie, slide expiry, re-issue Set-Cookie on touch
        .layer(from_fn_with_state(
            session_repo.clone(),
            sliding_session_middleware,
        ))
        // Repos via Extension layer so extractors can pull them
        .layer(Extension(session_repo))
        .layer(Extension(user_repo))
```

If `sliding_session_middleware` is not already re-exported from `crate::middleware`, add it to `src/middleware/mod.rs` (search for `pub use` and add `sliding_session_middleware` / `ValidatedSession`).

- [ ] **Step 6.2: Ensure `SessionRepository` is `Clone` and `'static`**

Already `Clone` (confirmed at `src/repositories/session_repo.rs:8`). No change needed.

- [ ] **Step 6.3: Build**

Run: `pwd && cargo check`
Expected: clean.

- [ ] **Step 6.4: Stage**

Run: `git add src/routes.rs src/middleware/mod.rs`

---

### Task 7: Integration tests for sliding behavior

**Files:**
- Modify: `tests/auth_test.rs`

- [ ] **Step 7.1: Add three integration tests at the end of `tests/auth_test.rs`**

```rust
#[tokio::test]
async fn test_sliding_session_reissues_cookie_when_throttle_elapsed() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    // Artificially age last_touched_at so the next request slides expiry.
    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE sessions SET last_touched_at = datetime('now', '-2 hours') WHERE token = ?",
            [&token],
        )
        .unwrap();
    }

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should reach the dashboard (no redirect).
    assert_ne!(response.status(), StatusCode::SEE_OTHER);

    // And Set-Cookie should have been re-issued with a fresh Max-Age.
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("sliding session should set cookie on touch")
        .to_str()
        .unwrap();
    assert!(set_cookie.starts_with("session="));
    assert!(set_cookie.contains("Max-Age=604800")); // 7 days in seconds
}

#[tokio::test]
async fn test_sliding_session_no_cookie_when_within_throttle() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    // Fresh session: last_touched_at is ~now, so within throttle.
    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers().get(header::SET_COOKIE).is_none(),
        "cookie should NOT be re-issued within throttle window"
    );
}

#[tokio::test]
async fn test_expired_session_redirects_to_login() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
            [&token],
        )
        .unwrap();
    }

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}
```

- [ ] **Step 7.2: Run tests**

Run: `pwd && cargo nextest run -E 'test(sliding) + test(expired_session_redirects)'`
Expected: all three pass.

- [ ] **Step 7.3: Run the full test suite**

Run: `pwd && cargo nextest run`
Expected: all tests pass.

- [ ] **Step 7.4: Stage**

Run: `git add tests/auth_test.rs`

---

### Task 8: Format, final check, commit C1

- [ ] **Step 8.1: Format**

Run: `pwd && cargo fmt`

- [ ] **Step 8.2: Verify staged files list**

Run: `git status --short`
Expected to include (and only include):
- `A  migrations/010_rebuild_sessions_with_last_touched_at.sql`
- `M  src/migrations.rs`
- `M  src/session.rs`
- `M  src/repositories/session_repo.rs`
- `M  src/middleware/auth.rs`
- `M  src/middleware/mod.rs` (if re-exports needed)
- `M  src/routes.rs`
- `M  tests/auth_test.rs`

If `cargo fmt` modified other files incidentally, stage them too with explicit paths.

- [ ] **Step 8.3: Final test run before commit**

Run: `pwd && cargo nextest run`
Expected: all tests pass.

- [ ] **Step 8.4: Commit C1**

Run:
```bash
git commit -m "$(cat <<'EOF'
feat(auth): add sliding session with idle TTL

Sessions now use a 7-day idle TTL that slides on every request outside
a 1-hour write throttle. A new axum middleware validates the cookie,
slides last_touched_at + expires_at, and re-issues the session cookie
with a fresh Max-Age on touched responses. AuthUser / OptionalAuthUser
read the validated session from request extensions instead of hitting
the database themselves.

Migration 010 rebuilds the sessions table to add last_touched_at NOT
NULL; pre-existing rows backfill from created_at so no active user is
logged out.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 8.5: Confirm commit**

Run: `git log --oneline -3`
Expected: the new `feat(auth)` commit at the top.

---

## Phase C2 — Logout Other Devices (Commit 2)

### Task 9: Add `list_for_user` to session repo (TDD)

**Files:**
- Modify: `src/repositories/session_repo.rs`

**Contract:**

```rust
pub struct SessionListRow {
    pub token: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_touched_at: chrono::DateTime<chrono::Utc>,
}

impl SessionRepository {
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionListRow>>;
}
```

Rows with `expires_at <= now` are filtered out. Order is `last_touched_at DESC`.

- [ ] **Step 9.1: Write failing test**

Append to the `mod tests` block in `src/repositories/session_repo.rs`:

```rust
    #[tokio::test]
    async fn test_list_for_user_returns_sessions_newest_first() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let t_old = repo.create(&user_id).await.unwrap();
        let t_mid = repo.create(&user_id).await.unwrap();
        let t_new = repo.create(&user_id).await.unwrap();

        // Stagger last_touched_at.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET last_touched_at = datetime('now', '-3 days') WHERE token = ?",
                [&t_old],
            )
            .unwrap();
            conn.execute(
                "UPDATE sessions SET last_touched_at = datetime('now', '-1 day') WHERE token = ?",
                [&t_mid],
            )
            .unwrap();
        }

        let rows = repo.list_for_user(&user_id).await.unwrap();
        let tokens: Vec<_> = rows.iter().map(|r| r.token.as_str()).collect();
        assert_eq!(tokens, vec![t_new.as_str(), t_mid.as_str(), t_old.as_str()]);
    }

    #[tokio::test]
    async fn test_list_for_user_filters_expired() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let live = repo.create(&user_id).await.unwrap();
        let dead = repo.create(&user_id).await.unwrap();
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 minute') WHERE token = ?",
                [&dead],
            )
            .unwrap();
        }

        let rows = repo.list_for_user(&user_id).await.unwrap();
        let tokens: Vec<_> = rows.iter().map(|r| r.token.as_str()).collect();
        assert_eq!(tokens, vec![live.as_str()]);
    }
```

- [ ] **Step 9.2: Run to verify failure**

Run: `pwd && cargo nextest run -E 'test(list_for_user)'`
Expected: compile errors about `list_for_user` / `SessionListRow`.

- [ ] **Step 9.3: Implement**

Add to `src/repositories/session_repo.rs` (above the `#[cfg(test)]` block):

```rust
pub struct SessionListRow {
    pub token: String,
    pub created_at: chrono::DateTime<Utc>,
    pub last_touched_at: chrono::DateTime<Utc>,
}

impl SessionRepository {
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionListRow>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let now = Utc::now();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT token, created_at, last_touched_at FROM sessions \
                 WHERE user_id = ? AND expires_at > ? \
                 ORDER BY last_touched_at DESC",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![user_id, now], |row| {
                    Ok(SessionListRow {
                        token: row.get(0)?,
                        created_at: row.get(1)?,
                        last_touched_at: row.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }
}
```

Important: wrap this in a fresh `impl SessionRepository { ... }` block if the existing block has already closed, or inline at the end of the existing impl block — whichever fits the file shape after Task 3's edits.

- [ ] **Step 9.4: Run to verify pass**

Run: `pwd && cargo nextest run -E 'test(list_for_user)'`
Expected: both tests pass.

- [ ] **Step 9.5: Stage**

Run: `git add src/repositories/session_repo.rs`

---

### Task 10: Settings handler — list sessions + logout-others

**Files:**
- Modify: `src/handlers/settings.rs`

- [ ] **Step 10.1: Update template struct and `index` handler**

In `src/handlers/settings.rs`, extend the `SettingsTemplate` struct and all its existing initialisers:

```rust
use crate::repositories::session_repo::SessionListRow;

#[derive(Template)]
#[template(path = "settings/index.html")]
struct SettingsTemplate {
    user: AuthUser,
    git_version: &'static str,
    error: Option<String>,
    success: Option<String>,
    sessions: Vec<SessionListRow>,
    current_token: String,
}
```

Update every place that constructs a `SettingsTemplate` (the `index` handler and all four in `change_password`) to include the two new fields. The `index` handler becomes:

```rust
pub async fn index(
    State(state): State<SettingsState>,
    auth_user: AuthUser,
) -> Result<Response> {
    let sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    let current_token = auth_user.session_token.clone();
    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: None,
        sessions,
        current_token,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
```

For `change_password`, the new fields are populated the same way — extract into a helper:

```rust
async fn load_sessions_for_template(
    session_repo: &SessionRepository,
    auth_user: &AuthUser,
) -> Result<Vec<SessionListRow>> {
    session_repo.list_for_user(&auth_user.id).await
}
```

and refactor `change_password` so each of its four `SettingsTemplate { ... }` constructors runs `load_sessions_for_template` once at the top and reuses the resulting `Vec`. Example shape (illustrative; adapt for each of the four branches):

```rust
pub async fn change_password(
    State(state): State<SettingsState>,
    auth_user: AuthUser,
    Form(form): Form<ChangePasswordForm>,
) -> Result<Response> {
    let sessions = load_sessions_for_template(&state.session_repo, &auth_user).await?;
    let current_token = auth_user.session_token.clone();

    let render = |user: AuthUser,
                  error: Option<String>,
                  success: Option<String>|
     -> Result<Response> {
        let template = SettingsTemplate {
            user,
            git_version: GIT_VERSION,
            error,
            success,
            sessions: sessions.clone(),
            current_token: current_token.clone(),
        };
        Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response())
    };

    if form.new_password != form.confirm_password {
        return render(auth_user, Some("New passwords do not match".to_string()), None);
    }
    if form.new_password.len() < 6 {
        return render(
            auth_user,
            Some("New password must be at least 6 characters".to_string()),
            None,
        );
    }
    let verified = state
        .user_repo
        .verify_password(&auth_user.username, &form.current_password)
        .await?;
    if verified.is_none() {
        return render(
            auth_user,
            Some("Current password is incorrect".to_string()),
            None,
        );
    }
    state
        .user_repo
        .change_password(&auth_user.id, &form.new_password)
        .await?;
    state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;

    // Session list should reflect the just-revoked sessions, so reload.
    let fresh_sessions = load_sessions_for_template(&state.session_repo, &auth_user).await?;
    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: Some(
            "Password changed successfully. All other sessions have been logged out.".to_string(),
        ),
        sessions: fresh_sessions,
        current_token,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
```

- [ ] **Step 10.2: Add `logout_others` handler**

Append to `src/handlers/settings.rs`:

```rust
pub async fn logout_others(
    State(state): State<SettingsState>,
    auth_user: AuthUser,
) -> Result<Response> {
    state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;

    let sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    let current_token = auth_user.session_token.clone();
    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: Some("Logged out of all other devices.".to_string()),
        sessions,
        current_token,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
```

- [ ] **Step 10.3: Build**

Run: `pwd && cargo check`
Expected: template field mismatches will surface if the template file hasn't been updated. Fix in Task 11. Staging continues.

- [ ] **Step 10.4: Stage**

Run: `git add src/handlers/settings.rs`

---

### Task 11: Settings template — active sessions section

**Files:**
- Modify: `templates/settings/index.html`

- [ ] **Step 11.1: Add the "Active Sessions" block**

Insert the following block in `templates/settings/index.html`, after the `Change Password` form (after line 37 `</form>`) and before `<h2>Application Info</h2>` (line 39):

```html
    <h2>Active Sessions</h2>
    <table class="data-table">
        <thead>
            <tr>
                <th>Device</th>
                <th>Last active</th>
                <th>Signed in</th>
            </tr>
        </thead>
        <tbody>
            {% for s in sessions %}
            <tr>
                <td>
                    {% if s.token == current_token %}
                    <strong>This device</strong>
                    {% else %}
                    Other device
                    {% endif %}
                </td>
                <td>{{ s.last_touched_at.format("%Y-%m-%d %H:%M UTC") }}</td>
                <td>{{ s.created_at.format("%Y-%m-%d %H:%M UTC") }}</td>
            </tr>
            {% endfor %}
        </tbody>
    </table>

    <form method="post" action="/settings/logout-others"
          onsubmit="return confirm('Log out of all other devices?');"
          style="margin-top: var(--sp-4);">
        <button type="submit">Log out all other devices</button>
    </form>
```

- [ ] **Step 11.2: Build**

Run: `pwd && cargo check`
Expected: no template / field errors.

- [ ] **Step 11.3: Stage**

Run: `git add templates/settings/index.html`

---

### Task 12: Register `POST /settings/logout-others` route

**Files:**
- Modify: `src/routes.rs`

- [ ] **Step 12.1: Add the route**

In `src/routes.rs`, extend the Settings block (`src/routes.rs:82-85` as of main). Change:

```rust
        // Settings routes
        .route("/settings", get(settings::index))
        .route("/settings/password", post(settings::change_password))
        .with_state(settings_state)
```

to:

```rust
        // Settings routes
        .route("/settings", get(settings::index))
        .route("/settings/password", post(settings::change_password))
        .route("/settings/logout-others", post(settings::logout_others))
        .with_state(settings_state)
```

- [ ] **Step 12.2: Build**

Run: `pwd && cargo check`
Expected: clean build.

- [ ] **Step 12.3: Stage**

Run: `git add src/routes.rs`

---

### Task 13: Integration tests for logout-others

**Files:**
- Modify: `tests/settings_test.rs` (this file already hosts `/settings/*` integration tests).

- [ ] **Step 13.1: Add integration tests**

Append:

```rust
#[tokio::test]
async fn test_settings_page_lists_sessions_with_this_device_marker() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let current_token = session_repo.create(&user.id).await.unwrap();
    let _other_token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, format!("session={}", current_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("Active Sessions"), "missing Active Sessions heading");
    assert!(body.contains("This device"), "missing This device marker");
    assert!(body.contains("Other device"), "missing Other device row");
}

#[tokio::test]
async fn test_logout_others_deletes_siblings_only() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let current_token = session_repo.create(&user.id).await.unwrap();
    let sibling_token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/logout-others")
                .header(header::COOKIE, format!("session={}", current_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Sibling is gone, current survives.
    assert!(
        session_repo.validate_and_touch(&sibling_token).await.unwrap().is_none(),
        "sibling session should be deleted"
    );
    assert!(
        session_repo.validate_and_touch(&current_token).await.unwrap().is_some(),
        "current session should survive"
    );
}

#[tokio::test]
async fn test_logout_others_form_has_confirm_attr() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(
        body.contains("onsubmit=\"return confirm("),
        "logout-others form should carry a confirm() guard"
    );
}
```

- [ ] **Step 13.2: Run the new tests**

Run: `pwd && cargo nextest run -E 'test(settings_page_lists_sessions) + test(logout_others)'`
Expected: all pass.

- [ ] **Step 13.3: Run the full suite**

Run: `pwd && cargo nextest run`
Expected: green.

- [ ] **Step 13.4: Stage**

Run: `git add tests/settings_test.rs`

(Confirm the imports at the top of `tests/settings_test.rs` already bring in `Body`, `Request`, `StatusCode`, `header`, `ServiceExt`, `BodyExt`, `UserRole`, and `common`. If not, add them to match `tests/auth_test.rs`.)

---

### Task 14: Format, final check, commit C2

- [ ] **Step 14.1: Format**

Run: `pwd && cargo fmt`

- [ ] **Step 14.2: Confirm staged files**

Run: `git status --short`
Expected:
- `M  src/repositories/session_repo.rs`
- `M  src/handlers/settings.rs`
- `M  templates/settings/index.html`
- `M  src/routes.rs`
- `M  tests/settings_test.rs`

- [ ] **Step 14.3: Final test run**

Run: `pwd && cargo nextest run`
Expected: all tests pass.

- [ ] **Step 14.4: Commit C2**

Run:
```bash
git commit -m "$(cat <<'EOF'
feat(settings): add "log out all other devices"

Settings page now shows the user's active sessions (created_at and
last_touched_at only — no User-Agent, no IP) with a "this device"
marker. A new POST /settings/logout-others endpoint reuses
delete_all_for_user_except to revoke siblings, rendered back through
the settings template with a success flash. The form uses a native
confirm() guard — no dedicated confirm page.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 14.5: Confirm**

Run: `git log --oneline -5`
Expected: both `feat(auth)` (C1) and `feat(settings)` (C2) commits on top of the spec commit `1e8cd2c`.

---

## Post-Implementation Checklist

- [ ] `pwd && cargo nextest run` — all green
- [ ] `pwd && cargo fmt -- --check` — clean
- [ ] `pwd && cargo clippy --all-targets -- -D warnings` — clean (if the repo uses clippy-in-CI; optional otherwise)
- [ ] `git log --oneline feat/sliding-session ^main` shows exactly two implementation commits plus the spec commit (three total)
- [ ] Manual smoke test recommended: run `cargo run`, log in, leave the tab open for > 1 hour, make any request, confirm the session cookie was re-issued (DevTools → Application → Cookies → `session` → `Expires`).
- [ ] Manual smoke test: log in from two browsers, click "Log out all other devices" in one, confirm the other is redirected to `/auth/login` on its next request.
