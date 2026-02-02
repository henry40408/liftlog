use chrono::{NaiveDate, Utc};
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::{
    DynamicPR, FromSqliteRow, WorkoutLog, WorkoutLogWithExercise, WorkoutSession,
};

#[derive(Clone)]
pub struct WorkoutRepository {
    pool: DbPool,
}

impl WorkoutRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    // Workout Sessions
    pub async fn create_session(
        &self,
        user_id: &str,
        date: NaiveDate,
        notes: Option<&str>,
    ) -> Result<WorkoutSession> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let session = WorkoutSession {
            id: id.clone(),
            user_id: user_id.to_string(),
            date,
            notes: notes.map(|s| s.to_string()),
            created_at: now,
        };
        let session_clone = session.clone();

        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO workout_sessions (id, user_id, date, notes, created_at) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![
                    session_clone.id,
                    session_clone.user_id,
                    session_clone.date,
                    session_clone.notes,
                    session_clone.created_at
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

        Ok(session)
    }

    pub async fn find_session_by_id(&self, id: &str) -> Result<Option<WorkoutSession>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM workout_sessions WHERE id = ?")?;
            let result = stmt.query_row([&id], WorkoutSession::from_row).optional()?;
            Ok(result)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    #[allow(dead_code)]
    pub async fn find_sessions_by_user(&self, user_id: &str) -> Result<Vec<WorkoutSession>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn
                .prepare("SELECT * FROM workout_sessions WHERE user_id = ? ORDER BY date DESC")?;
            let sessions = stmt
                .query_map([&user_id], WorkoutSession::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(sessions)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn find_sessions_by_user_paginated(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<WorkoutSession>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT * FROM workout_sessions WHERE user_id = ? ORDER BY date DESC LIMIT ? OFFSET ?"
            )?;
            let sessions = stmt
                .query_map(rusqlite::params![user_id, limit, offset], WorkoutSession::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(sessions)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn count_sessions_by_user(&self, user_id: &str) -> Result<i64> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM workout_sessions WHERE user_id = ?",
                [&user_id],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn update_session(
        &self,
        id: &str,
        user_id: &str,
        date: Option<NaiveDate>,
        notes: Option<&str>,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let user_id = user_id.to_string();
        let notes = notes.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = if let Some(d) = date {
                conn.execute(
                    "UPDATE workout_sessions SET date = ?, notes = ? WHERE id = ? AND user_id = ?",
                    rusqlite::params![d, notes, id, user_id],
                )?
            } else {
                conn.execute(
                    "UPDATE workout_sessions SET notes = ? WHERE id = ? AND user_id = ?",
                    rusqlite::params![notes, id, user_id],
                )?
            };
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn delete_session(&self, id: &str, user_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "DELETE FROM workout_sessions WHERE id = ? AND user_id = ?",
                rusqlite::params![id, user_id],
            )?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    // Workout Logs
    pub async fn create_log(
        &self,
        session_id: &str,
        exercise_id: &str,
        set_number: i32,
        reps: i32,
        weight: f64,
        rpe: Option<i32>,
    ) -> Result<WorkoutLog> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let log = WorkoutLog {
            id: id.clone(),
            session_id: session_id.to_string(),
            exercise_id: exercise_id.to_string(),
            set_number,
            reps,
            weight,
            rpe,
            created_at: now,
        };
        let log_clone = log.clone();

        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight, rpe, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    log_clone.id,
                    log_clone.session_id,
                    log_clone.exercise_id,
                    log_clone.set_number,
                    log_clone.reps,
                    log_clone.weight,
                    log_clone.rpe,
                    log_clone.created_at
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

        Ok(log)
    }

    /// Find logs by session with dynamically computed is_pr
    pub async fn find_logs_by_session_with_pr(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<Vec<WorkoutLogWithExercise>> {
        let pool = self.pool.clone();
        let session_id = session_id.to_string();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT wl.id, wl.session_id, wl.exercise_id, e.name as exercise_name,
                        wl.set_number, wl.reps, wl.weight, wl.rpe,
                        CASE WHEN wl.weight = (
                            SELECT MAX(wl2.weight) FROM workout_logs wl2
                            JOIN workout_sessions ws2 ON wl2.session_id = ws2.id
                            WHERE ws2.user_id = ? AND wl2.exercise_id = wl.exercise_id
                        ) THEN 1 ELSE 0 END as is_pr
                 FROM workout_logs wl
                 JOIN exercises e ON wl.exercise_id = e.id
                 WHERE wl.session_id = ?
                 ORDER BY wl.created_at, wl.set_number",
            )?;
            let logs = stmt
                .query_map(rusqlite::params![user_id, session_id], WorkoutLogWithExercise::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(logs)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    #[allow(dead_code)]
    pub async fn find_log_by_id(&self, id: &str) -> Result<Option<WorkoutLog>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM workout_logs WHERE id = ?")?;
            let result = stmt.query_row([&id], WorkoutLog::from_row).optional()?;
            Ok(result)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn delete_log(&self, id: &str, session_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let session_id = session_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "DELETE FROM workout_logs WHERE id = ? AND session_id = ?",
                rusqlite::params![id, session_id],
            )?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn update_log(
        &self,
        id: &str,
        session_id: &str,
        reps: i32,
        weight: f64,
        rpe: Option<i32>,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let session_id = session_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "UPDATE workout_logs SET reps = ?, weight = ?, rpe = ? WHERE id = ? AND session_id = ?",
                rusqlite::params![reps, weight, rpe, id, session_id],
            )?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn get_next_set_number(&self, session_id: &str, exercise_id: &str) -> Result<i32> {
        let pool = self.pool.clone();
        let session_id = session_id.to_string();
        let exercise_id = exercise_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let result: Option<i32> = conn
                .query_row(
                    "SELECT MAX(set_number) FROM workout_logs WHERE session_id = ? AND exercise_id = ?",
                    rusqlite::params![session_id, exercise_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            Ok(result.map(|n| n + 1).unwrap_or(1))
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    // Dynamic Personal Records

    /// Get all PRs for a user (one per exercise, max weight)
    pub async fn get_all_prs_by_user(&self, user_id: &str) -> Result<Vec<DynamicPR>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT wl.exercise_id, e.name as exercise_name,
                        MAX(wl.weight) as value,
                        (SELECT wl3.created_at FROM workout_logs wl3
                         JOIN workout_sessions ws3 ON wl3.session_id = ws3.id
                         WHERE ws3.user_id = ? AND wl3.exercise_id = wl.exercise_id
                         ORDER BY wl3.weight DESC, wl3.created_at DESC LIMIT 1) as achieved_at
                 FROM workout_logs wl
                 JOIN workout_sessions ws ON wl.session_id = ws.id
                 JOIN exercises e ON wl.exercise_id = e.id
                 WHERE ws.user_id = ?
                 GROUP BY wl.exercise_id
                 ORDER BY achieved_at DESC",
            )?;
            let prs = stmt
                .query_map(rusqlite::params![user_id, user_id], DynamicPR::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(prs)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// Get the max weight PR for a specific exercise
    pub async fn get_max_weight_for_exercise(
        &self,
        user_id: &str,
        exercise_id: &str,
    ) -> Result<Option<DynamicPR>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let exercise_id = exercise_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT wl.exercise_id, e.name as exercise_name,
                        MAX(wl.weight) as value, wl.created_at as achieved_at
                 FROM workout_logs wl
                 JOIN workout_sessions ws ON wl.session_id = ws.id
                 JOIN exercises e ON wl.exercise_id = e.id
                 WHERE ws.user_id = ? AND wl.exercise_id = ?
                 GROUP BY wl.exercise_id",
            )?;
            let result = stmt
                .query_row(rusqlite::params![user_id, exercise_id], DynamicPR::from_row)
                .optional()?;
            Ok(result)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    // Statistics
    pub async fn count_workouts_this_week(&self, user_id: &str) -> Result<i64> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM workout_sessions
                 WHERE user_id = ? AND date >= date('now', '-7 days')",
                [&user_id],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn count_workouts_this_month(&self, user_id: &str) -> Result<i64> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM workout_sessions
                 WHERE user_id = ? AND date >= date('now', '-30 days')",
                [&user_id],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn get_total_volume_this_week(&self, user_id: &str) -> Result<f64> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let result: Option<f64> = conn
                .query_row(
                    "SELECT SUM(wl.weight * wl.reps)
                     FROM workout_logs wl
                     JOIN workout_sessions ws ON wl.session_id = ws.id
                     WHERE ws.user_id = ? AND ws.date >= date('now', '-7 days')",
                    [&user_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            Ok(result.unwrap_or(0.0))
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// Get exercise history with dynamically computed is_pr
    pub async fn get_exercise_history_with_pr(
        &self,
        user_id: &str,
        exercise_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkoutLogWithExercise>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let exercise_id = exercise_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT wl.id, wl.session_id, wl.exercise_id, e.name as exercise_name,
                        wl.set_number, wl.reps, wl.weight, wl.rpe,
                        CASE WHEN wl.weight = (
                            SELECT MAX(wl2.weight) FROM workout_logs wl2
                            JOIN workout_sessions ws2 ON wl2.session_id = ws2.id
                            WHERE ws2.user_id = ? AND wl2.exercise_id = wl.exercise_id
                        ) THEN 1 ELSE 0 END as is_pr
                 FROM workout_logs wl
                 JOIN workout_sessions ws ON wl.session_id = ws.id
                 JOIN exercises e ON wl.exercise_id = e.id
                 WHERE ws.user_id = ? AND wl.exercise_id = ?
                 ORDER BY ws.date DESC, wl.set_number
                 LIMIT ?",
            )?;
            let logs = stmt
                .query_map(
                    rusqlite::params![user_id, user_id, exercise_id, limit],
                    WorkoutLogWithExercise::from_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(logs)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_memory_pool;
    use crate::migrations::run_migrations_for_tests;

    fn setup_test_db() -> DbPool {
        let pool = create_memory_pool().expect("Failed to create test database");
        run_migrations_for_tests(&pool).expect("Failed to run migrations");
        pool
    }

    fn create_test_user(pool: &DbPool, user_id: &str) {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, datetime('now'))",
            rusqlite::params![user_id, format!("user_{}", user_id), "hash", "user"],
        ).unwrap();
    }

    fn create_test_exercise(pool: &DbPool, exercise_id: &str, user_id: &str) {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO exercises (id, name, category, user_id)
             VALUES (?, ?, ?, ?)",
            rusqlite::params![exercise_id, "Test Exercise", "chest", user_id],
        ).unwrap();
    }

    // Workout Session Tests

    #[tokio::test]
    async fn test_create_session() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo
            .create_session("user1", date, Some("Leg day"))
            .await
            .unwrap();

        assert_eq!(session.user_id, "user1");
        assert_eq!(session.date, date);
        assert_eq!(session.notes, Some("Leg day".to_string()));
        assert!(!session.id.is_empty());
    }

    #[tokio::test]
    async fn test_find_session_by_id_exists() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let created = repo.create_session("user1", date, None).await.unwrap();
        let found = repo.find_session_by_id(&created.id).await.unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_find_session_by_id_not_exists() {
        let pool = setup_test_db();
        let repo = WorkoutRepository::new(pool);

        let found = repo.find_session_by_id("nonexistent").await.unwrap();

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_sessions_by_user_ordered() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = WorkoutRepository::new(pool);

        let date1 = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let date3 = NaiveDate::from_ymd_opt(2024, 1, 12).unwrap();

        repo.create_session("user1", date1, None).await.unwrap();
        repo.create_session("user1", date2, None).await.unwrap();
        repo.create_session("user1", date3, None).await.unwrap();

        let sessions = repo.find_sessions_by_user("user1").await.unwrap();

        assert_eq!(sessions.len(), 3);
        // Should be ordered by date DESC
        assert_eq!(sessions[0].date, date2);
        assert_eq!(sessions[1].date, date3);
        assert_eq!(sessions[2].date, date1);
    }

    #[tokio::test]
    async fn test_count_sessions_by_user() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_user(&pool, "user2");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        repo.create_session("user1", date, None).await.unwrap();
        repo.create_session("user1", date, None).await.unwrap();
        repo.create_session("user2", date, None).await.unwrap();

        let count = repo.count_sessions_by_user("user1").await.unwrap();

        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_delete_session_success() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();
        let deleted = repo.delete_session(&session.id, "user1").await.unwrap();

        assert!(deleted);
        let found = repo.find_session_by_id(&session.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_session_wrong_user() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_user(&pool, "user2");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();
        let deleted = repo.delete_session(&session.id, "user2").await.unwrap();

        assert!(!deleted);
    }

    // Workout Log Tests

    #[tokio::test]
    async fn test_create_log() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        let log = repo
            .create_log(&session.id, "ex-bench-press", 1, 10, 100.0, Some(8))
            .await
            .unwrap();

        assert_eq!(log.session_id, session.id);
        assert_eq!(log.exercise_id, "ex-bench-press");
        assert_eq!(log.set_number, 1);
        assert_eq!(log.reps, 10);
        assert_eq!(log.weight, 100.0);
        assert_eq!(log.rpe, Some(8));
    }

    #[tokio::test]
    async fn test_find_logs_by_session_with_pr() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        create_test_exercise(&pool, "ex-squat", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();
        repo.create_log(&session.id, "ex-bench-press", 2, 8, 105.0, None)
            .await
            .unwrap();
        repo.create_log(&session.id, "ex-squat", 1, 5, 120.0, None)
            .await
            .unwrap();

        let logs = repo
            .find_logs_by_session_with_pr(&session.id, "user1")
            .await
            .unwrap();

        assert_eq!(logs.len(), 3);
        // 105.0 is PR for bench press, 120.0 is PR for squat
        assert!(!logs[0].is_pr); // 100.0 bench
        assert!(logs[1].is_pr); // 105.0 bench - PR
        assert!(logs[2].is_pr); // 120.0 squat - PR
    }

    #[tokio::test]
    async fn test_delete_log_success() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();
        let log = repo
            .create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();

        let deleted = repo.delete_log(&log.id, &session.id).await.unwrap();

        assert!(deleted);
        let found = repo.find_log_by_id(&log.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_update_log_success() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();
        let log = repo
            .create_log(&session.id, "ex-bench-press", 1, 10, 100.0, Some(7))
            .await
            .unwrap();

        let updated = repo
            .update_log(&log.id, &session.id, 12, 110.0, Some(8))
            .await
            .unwrap();

        assert!(updated);
        let found = repo.find_log_by_id(&log.id).await.unwrap().unwrap();
        assert_eq!(found.reps, 12);
        assert_eq!(found.weight, 110.0);
        assert_eq!(found.rpe, Some(8));
    }

    #[tokio::test]
    async fn test_update_log_wrong_session() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();
        let log = repo
            .create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();

        // Try to update with wrong session_id
        let updated = repo
            .update_log(&log.id, "wrong-session", 12, 110.0, Some(8))
            .await
            .unwrap();

        assert!(!updated);
        // Verify log was not modified
        let found = repo.find_log_by_id(&log.id).await.unwrap().unwrap();
        assert_eq!(found.reps, 10);
        assert_eq!(found.weight, 100.0);
    }

    #[tokio::test]
    async fn test_get_next_set_number() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        // First set should be 1
        let next = repo
            .get_next_set_number(&session.id, "ex-bench-press")
            .await
            .unwrap();
        assert_eq!(next, 1);

        // After creating a log, next should be 2
        repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();
        let next = repo
            .get_next_set_number(&session.id, "ex-bench-press")
            .await
            .unwrap();
        assert_eq!(next, 2);
    }

    // Dynamic Personal Record Tests

    #[tokio::test]
    async fn test_get_all_prs_by_user() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        create_test_exercise(&pool, "ex-squat", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();
        repo.create_log(&session.id, "ex-bench-press", 2, 8, 110.0, None)
            .await
            .unwrap();
        repo.create_log(&session.id, "ex-squat", 1, 5, 150.0, None)
            .await
            .unwrap();

        let prs = repo.get_all_prs_by_user("user1").await.unwrap();

        assert_eq!(prs.len(), 2);
        // Find each exercise's PR
        let bench_pr = prs.iter().find(|p| p.exercise_id == "ex-bench-press");
        let squat_pr = prs.iter().find(|p| p.exercise_id == "ex-squat");
        assert!(bench_pr.is_some());
        assert!(squat_pr.is_some());
        assert_eq!(bench_pr.unwrap().value, 110.0);
        assert_eq!(squat_pr.unwrap().value, 150.0);
    }

    #[tokio::test]
    async fn test_get_max_weight_for_exercise() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();
        repo.create_log(&session.id, "ex-bench-press", 2, 8, 110.0, None)
            .await
            .unwrap();
        repo.create_log(&session.id, "ex-bench-press", 3, 5, 105.0, None)
            .await
            .unwrap();

        let pr = repo
            .get_max_weight_for_exercise("user1", "ex-bench-press")
            .await
            .unwrap();

        assert!(pr.is_some());
        assert_eq!(pr.unwrap().value, 110.0);
    }

    #[tokio::test]
    async fn test_dynamic_pr_updates_when_heavier_set_added() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        // First set
        repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();

        let logs = repo
            .find_logs_by_session_with_pr(&session.id, "user1")
            .await
            .unwrap();
        assert!(logs[0].is_pr); // 100.0 is the only set, so it's PR

        // Add heavier set
        repo.create_log(&session.id, "ex-bench-press", 2, 8, 110.0, None)
            .await
            .unwrap();

        let logs = repo
            .find_logs_by_session_with_pr(&session.id, "user1")
            .await
            .unwrap();
        assert!(!logs[0].is_pr); // 100.0 is no longer PR
        assert!(logs[1].is_pr); // 110.0 is now PR
    }

    #[tokio::test]
    async fn test_dynamic_pr_updates_when_pr_deleted() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_exercise(&pool, "ex-bench-press", "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
            .await
            .unwrap();
        let heavy_log = repo
            .create_log(&session.id, "ex-bench-press", 2, 8, 110.0, None)
            .await
            .unwrap();

        // Delete the PR set
        repo.delete_log(&heavy_log.id, &session.id).await.unwrap();

        let logs = repo
            .find_logs_by_session_with_pr(&session.id, "user1")
            .await
            .unwrap();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].is_pr); // 100.0 becomes PR again
    }

    #[tokio::test]
    async fn test_update_session() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = WorkoutRepository::new(pool);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let session = repo.create_session("user1", date, None).await.unwrap();

        let new_date = NaiveDate::from_ymd_opt(2024, 1, 20).unwrap();
        let updated = repo
            .update_session(&session.id, "user1", Some(new_date), Some("Updated notes"))
            .await
            .unwrap();

        assert!(updated);

        let found = repo.find_session_by_id(&session.id).await.unwrap().unwrap();
        assert_eq!(found.date, new_date);
        assert_eq!(found.notes, Some("Updated notes".to_string()));
    }

    #[tokio::test]
    async fn test_find_sessions_by_user_paginated() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = WorkoutRepository::new(pool);

        for i in 1..=5 {
            let date = NaiveDate::from_ymd_opt(2024, 1, i).unwrap();
            repo.create_session("user1", date, None).await.unwrap();
        }

        let page1 = repo
            .find_sessions_by_user_paginated("user1", 2, 0)
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = repo
            .find_sessions_by_user_paginated("user1", 2, 2)
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = repo
            .find_sessions_by_user_paginated("user1", 2, 4)
            .await
            .unwrap();
        assert_eq!(page3.len(), 1);
    }
}
