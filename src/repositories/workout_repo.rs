use chrono::{NaiveDate, Utc};
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::{
    FromSqliteRow, PersonalRecord, PersonalRecordWithExercise, WorkoutLog, WorkoutLogWithExercise,
    WorkoutSession,
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
            is_pr: false,
            created_at: now,
        };
        let log_clone = log.clone();

        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight, rpe, is_pr, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)",
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

    pub async fn find_logs_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<WorkoutLogWithExercise>> {
        let pool = self.pool.clone();
        let session_id = session_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT wl.id, wl.session_id, wl.exercise_id, e.name as exercise_name,
                        wl.set_number, wl.reps, wl.weight, wl.rpe, wl.is_pr
                 FROM workout_logs wl
                 JOIN exercises e ON wl.exercise_id = e.id
                 WHERE wl.session_id = ?
                 ORDER BY wl.created_at, wl.set_number",
            )?;
            let logs = stmt
                .query_map([&session_id], WorkoutLogWithExercise::from_row)?
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

    pub async fn mark_as_pr(&self, id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute("UPDATE workout_logs SET is_pr = 1 WHERE id = ?", [&id])?;
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

    // Personal Records
    pub async fn find_pr(
        &self,
        user_id: &str,
        exercise_id: &str,
        record_type: &str,
    ) -> Result<Option<PersonalRecord>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let exercise_id = exercise_id.to_string();
        let record_type = record_type.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT * FROM personal_records WHERE user_id = ? AND exercise_id = ? AND record_type = ?"
            )?;
            let result = stmt
                .query_row(
                    rusqlite::params![user_id, exercise_id, record_type],
                    PersonalRecord::from_row,
                )
                .optional()?;
            Ok(result)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn upsert_pr(
        &self,
        user_id: &str,
        exercise_id: &str,
        record_type: &str,
        value: f64,
    ) -> Result<PersonalRecord> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let pr = PersonalRecord {
            id: id.clone(),
            user_id: user_id.to_string(),
            exercise_id: exercise_id.to_string(),
            record_type: record_type.to_string(),
            value,
            achieved_at: now,
        };
        let pr_clone = pr.clone();

        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO personal_records (id, user_id, exercise_id, record_type, value, achieved_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(user_id, exercise_id, record_type)
                 DO UPDATE SET value = excluded.value, achieved_at = excluded.achieved_at",
                rusqlite::params![
                    pr_clone.id,
                    pr_clone.user_id,
                    pr_clone.exercise_id,
                    pr_clone.record_type,
                    pr_clone.value,
                    pr_clone.achieved_at
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

        Ok(pr)
    }

    pub async fn find_prs_by_user(&self, user_id: &str) -> Result<Vec<PersonalRecordWithExercise>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT pr.id, pr.user_id, pr.exercise_id, e.name as exercise_name,
                        pr.record_type, pr.value, pr.achieved_at
                 FROM personal_records pr
                 JOIN exercises e ON pr.exercise_id = e.id
                 WHERE pr.user_id = ?
                 ORDER BY pr.achieved_at DESC",
            )?;
            let prs = stmt
                .query_map([&user_id], PersonalRecordWithExercise::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(prs)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn find_prs_by_exercise(
        &self,
        user_id: &str,
        exercise_id: &str,
    ) -> Result<Vec<PersonalRecord>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let exercise_id = exercise_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT * FROM personal_records
                 WHERE user_id = ? AND exercise_id = ?
                 ORDER BY record_type",
            )?;
            let prs = stmt
                .query_map(
                    rusqlite::params![user_id, exercise_id],
                    PersonalRecord::from_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(prs)
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

    pub async fn get_exercise_history(
        &self,
        user_id: &str,
        exercise_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkoutLog>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let exercise_id = exercise_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT wl.* FROM workout_logs wl
                 JOIN workout_sessions ws ON wl.session_id = ws.id
                 WHERE ws.user_id = ? AND wl.exercise_id = ?
                 ORDER BY ws.date DESC, wl.set_number
                 LIMIT ?",
            )?;
            let logs = stmt
                .query_map(
                    rusqlite::params![user_id, exercise_id, limit],
                    WorkoutLog::from_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(logs)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }
}
