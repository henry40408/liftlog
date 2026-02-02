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
        "005_create_personal_records.sql",
        include_str!("../migrations/005_create_personal_records.sql"),
    ),
    (
        "007_add_user_role.sql",
        include_str!("../migrations/007_add_user_role.sql"),
    ),
];

/// Run all pending migrations on the database pool.
///
/// This function tracks which migrations have been applied in a `_migrations` table
/// and only runs migrations that haven't been applied yet.
pub fn run_migrations(pool: &DbPool) -> anyhow::Result<()> {
    tracing::info!("Running migrations...");

    let conn = pool.get()?;

    // Create migrations tracking table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    for (filename, sql) in MIGRATIONS {
        // Check if migration was already applied
        let already_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = ?",
                [filename],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if already_applied {
            tracing::debug!("Skipping already applied migration: {}", filename);
            continue;
        }

        tracing::info!("Running migration: {}", filename);

        conn.execute_batch(sql)?;

        // Record that migration was applied
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
