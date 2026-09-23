//! SQL migrations embedded into the binary.

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

/// Applies migrations not yet recorded in `_migrations`.
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

    // 010 and 011 turn foreign_keys off and don't restore it; this pooled
    // connection would otherwise serve later requests unenforced.
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    // Warn, don't abort: orphans are harmless (validate_and_touch INNER JOINs
    // users), a startup lockout is not.
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

/// Applies every migration without tracking, for fresh in-memory test DBs.
#[allow(dead_code)] // Used by integration tests
pub fn run_migrations_for_tests(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;

    for (_filename, sql) in MIGRATIONS {
        conn.execute_batch(sql)?;
    }

    // As in `run_migrations`; the max_size(1) test pool reuses this connection.
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
        // max_size(1): schema, orphan inserts and cleanup share one connection.
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(manager)
            .expect("raw memory pool");

        let conn = pool.get().unwrap();

        // Schema up to (not including) 011, located by name.
        let cleanup_idx = MIGRATIONS
            .iter()
            .position(|(name, _)| *name == "011_cleanup_orphan_rows.sql")
            .expect("011 is registered");
        for (_filename, sql) in &MIGRATIONS[..cleanup_idx] {
            conn.execute_batch(sql).unwrap();
        }

        // Explicit, so the orphan inserts don't rely on 010's side effect.
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
        // An existing DB skips 010, so 011 must work without 010 having just
        // turned foreign_keys off.
        let pool = create_memory_pool().expect("memory pool");
        let conn = pool.get().unwrap();

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

        // Record 001–010 as applied, so `run_migrations` starts at 011.
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

        // Off only to insert the orphans below.
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

        // A log under a real session but the orphan exercise: orphaned only
        // once the exercise is deleted, so 011 must delete parents first.
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

        // max_size(1): release it or `run_migrations` blocks forever.
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
