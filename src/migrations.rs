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
        // create_memory_pool() now enables PRAGMA foreign_keys, which would
        // block inserting the orphan rows this test needs to set up. Build a
        // raw pool without that pragma instead. Note max_size(1): every
        // pool.get() below hands back the same physical connection, so the
        // schema, the orphan inserts, and the cleanup migration all land on
        // one connection with no risk of a second connection missing state.
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(manager)
            .expect("raw memory pool");

        let conn = pool.get().unwrap();

        // Build the schema with every migration except the final cleanup
        // one, so orphans can still be inserted afterwards.
        for (_filename, sql) in &MIGRATIONS[..MIGRATIONS.len() - 1] {
            conn.execute_batch(sql).unwrap();
        }

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

        let (_filename, cleanup_sql) = MIGRATIONS.last().expect("011 is registered");
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
}
