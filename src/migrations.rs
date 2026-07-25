//! Embedded database migrations
//!
//! This module contains all SQL migrations embedded into the binary,
//! eliminating the need for external migration files at runtime.

use crate::db::DbPool;

/// All migrations in order, each as (filename, `sql_content`)
pub const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001_create_users.sql",
        include_str!("../migrations/001_create_users.sql"),
    ),
    (
        "002_create_exercises.sql",
        include_str!("../migrations/002_create_exercises.sql"),
    ),
    (
        "003_create_workout_sessions.sql",
        include_str!("../migrations/003_create_workout_sessions.sql"),
    ),
    (
        "004_create_workout_logs.sql",
        include_str!("../migrations/004_create_workout_logs.sql"),
    ),
    (
        "007_add_user_role.sql",
        include_str!("../migrations/007_add_user_role.sql"),
    ),
    (
        "008_create_sessions.sql",
        include_str!("../migrations/008_create_sessions.sql"),
    ),
    (
        "009_add_workout_share_token.sql",
        include_str!("../migrations/009_add_workout_share_token.sql"),
    ),
    (
        "010_rebuild_sessions_with_last_touched_at.sql",
        include_str!("../migrations/010_rebuild_sessions_with_last_touched_at.sql"),
    ),
    (
        "011_cleanup_orphan_rows.sql",
        include_str!("../migrations/011_cleanup_orphan_rows.sql"),
    ),
    (
        "012_add_workout_share_expires_at.sql",
        include_str!("../migrations/012_add_workout_share_expires_at.sql"),
    ),
];

/// Run all pending migrations on the database pool.
///
/// This function tracks which migrations have been applied in a `_migrations` table
/// and only runs migrations that haven't been applied yet.
pub fn run_migrations(pool: &DbPool) -> anyhow::Result<()> {
    use std::collections::HashSet;

    tracing::info!("Running migrations...");

    let conn = pool.get()?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    let applied: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT name FROM _migrations")?;

        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<HashSet<String>>>()?
    };

    for (filename, sql) in MIGRATIONS {
        if applied.contains(*filename) {
            tracing::debug!("Skipping already applied migration: {}", filename);
            continue;
        }

        tracing::info!("Running migration: {}", filename);

        conn.execute_batch(sql)?;
        conn.execute("INSERT INTO _migrations (name) VALUES (?)", [filename])?;
    }

    // Migration 010 turns foreign_keys off for its table rebuild and
    // deliberately does not turn it back on itself (see that migration's
    // trailing comment) — only the pool's connection initialiser is allowed
    // to be the source of truth for every *other* connection. But this
    // particular connection came from the pool and returns to it once this
    // function ends, so without restoring the pragma here it would sit in
    // the pool carrying enforcement-off state and could be handed out to a
    // real request later, silently unenforced.
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    // Confirm 011 left no FK violations behind. Warn rather than abort:
    // locking an operator out of their own instance at startup is worse than
    // carrying orphan rows, and orphans are not an authentication risk —
    // validate_and_touch INNER JOINs users, so a session whose user is gone
    // simply fails to validate.
    {
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        let violations: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if !violations.is_empty() {
            tracing::warn!(
                count = violations.len(),
                ?violations,
                "PRAGMA foreign_key_check reported violations after migrations"
            );
        }
    }

    tracing::info!("Migrations completed");
    Ok(())
}

/// Run all migrations for tests (without tracking).
///
/// This is a simpler version that just runs all migrations without tracking,
/// suitable for in-memory test databases that are created fresh each time.
#[allow(dead_code)] // Used by integration tests
pub fn run_migrations_for_tests(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;

    for (_filename, sql) in MIGRATIONS {
        conn.execute_batch(sql)?;
    }

    // See the matching comment in `run_migrations`: migration 010 turns
    // foreign_keys off for its rebuild and does not restore it, so this
    // connection (reused by every later `pool.get()` call against a
    // max_size(1) test pool — see `create_memory_pool`) must have it
    // restored here or every test built on this helper would silently run
    // with enforcement off.
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_memory_pool;

    #[test]
    #[allow(clippy::cast_sign_loss, reason = "SQL COUNT(*) is always >= 0")]
    fn run_migrations_creates_tracking_table_and_records_each_migration() {
        let pool = create_memory_pool().expect("memory pool");
        run_migrations(&pool).expect("first run");

        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count as usize, MIGRATIONS.len());
    }

    #[test]
    #[allow(clippy::cast_sign_loss, reason = "SQL COUNT(*) is always >= 0")]
    fn run_migrations_is_idempotent() {
        let pool = create_memory_pool().expect("memory pool");
        run_migrations(&pool).expect("first run");
        // Second invocation must not re-apply or error; the HashSet path
        // should short-circuit each migration as already applied.
        run_migrations(&pool).expect("second run");

        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count as usize, MIGRATIONS.len());
    }

    #[test]
    fn foreign_key_check_is_clean_after_migrations() {
        let pool = create_memory_pool().expect("memory pool");
        run_migrations(&pool).expect("run migrations");

        let conn = pool.get().unwrap();
        let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
        let violation_count = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .count();
        assert_eq!(violation_count, 0);
    }

    #[test]
    fn cleanup_migration_removes_preexisting_orphans() {
        // The bundled SQLite in this build defaults PRAGMA foreign_keys to ON
        // (SQLITE_DEFAULT_FOREIGN_KEYS), so a bare memory connection would
        // block inserting the orphan rows this test needs to set up. Build a
        // raw pool and explicitly turn enforcement off below, after the
        // schema exists, rather than relying on create_memory_pool() (which
        // now turns it ON) or on migration 010 happening to leave it off as
        // a side effect. Note max_size(1): every pool.get() below hands back
        // the same physical connection, so the schema, the orphan inserts,
        // and the cleanup migration all land on one connection with no risk
        // of a second connection missing state.
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(manager)
            .expect("raw memory pool");

        let conn = pool.get().unwrap();

        // Locate 011 by name rather than assuming it's `MIGRATIONS.last()` —
        // that assumption broke once 012 was appended after it. Build the
        // schema with every migration up to (not including) the cleanup
        // one, so orphans can still be inserted afterwards.
        let cleanup_idx = MIGRATIONS
            .iter()
            .position(|(name, _)| *name == "011_cleanup_orphan_rows.sql")
            .expect("011 is registered");
        for (_filename, sql) in &MIGRATIONS[..cleanup_idx] {
            conn.execute_batch(sql).unwrap();
        }

        // Explicit, not incidental: this test's orphan inserts must not
        // depend on 011 itself (which now turns the pragma off internally)
        // or on migration 010's side effect.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();

        conn.execute(
            "INSERT INTO users (id, username, password_hash, created_at) \
             VALUES ('u1', 'u1', 'hash', datetime('now'))",
            [],
        )
        .unwrap();

        // Orphan session: user_id matches no users row.
        conn.execute(
            "INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) \
             VALUES ('tok1', 'ghost-user', datetime('now'), datetime('now', '+1 day'), datetime('now'))",
            [],
        )
        .unwrap();

        // Orphan workout_log: session_id matches no workout_sessions row.
        // (exercise_id must point at a real exercise; that FK isn't under
        // test here.)
        conn.execute(
            "INSERT INTO exercises (id, name, category, user_id) \
             VALUES ('ex1', 'Squat', 'legs', 'u1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight) \
             VALUES ('log1', 'ghost-session', 'ex1', 1, 5, 100.0)",
            [],
        )
        .unwrap();

        let (_filename, cleanup_sql) = &MIGRATIONS[cleanup_idx];
        conn.execute_batch(cleanup_sql).unwrap();

        let session_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE token = 'tok1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_count, 0, "orphan session should be removed");

        let log_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workout_logs WHERE id = 'log1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(log_count, 0, "orphan workout log should be removed");
    }

    #[test]
    fn cleanup_migration_clears_orphans_on_a_db_already_at_010() {
        // Exercises the path FIX 1 targets: a database that already has 010
        // applied causes `run_migrations` to skip 010 entirely (its filename
        // is already recorded in `_migrations`), so 011 must behave
        // correctly without 010 having run in the same batch to leave
        // foreign_keys off for it. `create_memory_pool`'s max_size(1) means
        // every `pool.get()` below returns the same physical connection, so
        // schema setup, the orphan inserts, and the real `run_migrations`
        // call all land on one connection with nothing to miss.
        let pool = create_memory_pool().expect("memory pool");
        let conn = pool.get().unwrap();

        // Apply 001 through 010 by filename, not by positional slicing — a
        // later migration inserted between them would silently break an
        // index assumption, exactly as it did for the test above.
        let idx_010 = MIGRATIONS
            .iter()
            .position(|(name, _)| *name == "010_rebuild_sessions_with_last_touched_at.sql")
            .expect("010 is registered");
        let applied_filenames: Vec<&str> = MIGRATIONS[..=idx_010]
            .iter()
            .map(|(name, _)| *name)
            .collect();
        for (_filename, sql) in &MIGRATIONS[..=idx_010] {
            conn.execute_batch(sql).unwrap();
        }

        // Record 001-010 as already applied, exactly as `_migrations` would
        // read on a real database that was upgraded through 010 in the past.
        // The real `run_migrations` call below will therefore skip all of
        // them — 010 in particular — and go straight to 011, the path that
        // was previously untested.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                name TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )
        .unwrap();
        for filename in &applied_filenames {
            conn.execute("INSERT INTO _migrations (name) VALUES (?)", [filename])
                .unwrap();
        }

        // create_memory_pool() enables foreign_keys by default; turn it off
        // to insert the orphan rows below. Migration 011 no longer cares
        // whether this connection enters it with the pragma on or off, since
        // it now sets the pragma itself — this is purely to let the test set
        // up an inconsistent state that a real, long-lived database could
        // have accumulated before 011 first shipped.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();

        conn.execute(
            "INSERT INTO users (id, username, password_hash, created_at) \
             VALUES ('u1', 'u1', 'hash', datetime('now'))",
            [],
        )
        .unwrap();

        // Orphan session: user_id matches no users row.
        conn.execute(
            "INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) \
             VALUES ('orphan-tok', 'ghost-user', datetime('now'), datetime('now', '+1 day'), datetime('now'))",
            [],
        )
        .unwrap();

        // Orphan workout_session: user_id matches no users row.
        conn.execute(
            "INSERT INTO workout_sessions (id, user_id, date, created_at) \
             VALUES ('orphan-ws', 'ghost-user2', '2024-01-01', datetime('now'))",
            [],
        )
        .unwrap();

        // Orphan exercise: user_id matches no users row.
        conn.execute(
            "INSERT INTO exercises (id, name, category, user_id) \
             VALUES ('orphan-ex', 'Squat', 'legs', 'ghost-user3')",
            [],
        )
        .unwrap();

        // A workout_log that references a *real* workout_session but the
        // orphan exercise above: it is not itself orphaned until the
        // exercise row is deleted. The old children-first ordering missed
        // exactly this case — it deleted workout_logs before the exercise
        // delete created this orphan, so the row survived forever.
        conn.execute(
            "INSERT INTO workout_sessions (id, user_id, date, created_at) \
             VALUES ('real-ws', 'u1', '2024-01-02', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight) \
             VALUES ('log1', 'real-ws', 'orphan-ex', 1, 5, 100.0)",
            [],
        )
        .unwrap();

        // The pool is max_size(1): `run_migrations` below needs to check out
        // that single connection itself, so the setup connection must be
        // released first or the call would block forever waiting for a
        // connection nothing will ever return.
        drop(conn);

        run_migrations(&pool).expect("run_migrations should not abort on pre-existing orphans");

        let conn = pool.get().unwrap();
        let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
        let violations = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .count();
        assert_eq!(violations, 0, "no foreign key violations should remain");

        for (table, id_col, id) in [
            ("sessions", "token", "orphan-tok"),
            ("workout_sessions", "id", "orphan-ws"),
            ("exercises", "id", "orphan-ex"),
            ("workout_logs", "id", "log1"),
        ] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE {id_col} = ?"),
                    [id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "{table} row {id} should have been cleaned up");
        }
    }
}
