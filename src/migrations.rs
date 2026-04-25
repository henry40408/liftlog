//! Embedded database migrations
//!
//! This module contains all SQL migrations embedded into the binary,
//! eliminating the need for external migration files at runtime.

use crate::db::DbPool;

/// All migrations in order, each as (filename, sql_content)
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
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<HashSet<String>>>()?;
        rows
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

    Ok(())
}
