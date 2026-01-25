use chrono::{NaiveDate, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    PersonalRecord, PersonalRecordWithExercise, WorkoutLog, WorkoutLogWithExercise, WorkoutSession,
};

#[derive(Clone)]
pub struct WorkoutRepository {
    pool: SqlitePool,
}

impl WorkoutRepository {
    pub fn new(pool: SqlitePool) -> Self {
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

        sqlx::query(
            "INSERT INTO workout_sessions (id, user_id, date, notes, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(user_id)
        .bind(date)
        .bind(notes)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(WorkoutSession {
            id,
            user_id: user_id.to_string(),
            date,
            notes: notes.map(|s| s.to_string()),
            created_at: now,
        })
    }

    pub async fn find_session_by_id(&self, id: &str) -> Result<Option<WorkoutSession>> {
        let session = sqlx::query_as::<_, WorkoutSession>(
            "SELECT * FROM workout_sessions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(session)
    }

    pub async fn find_sessions_by_user(&self, user_id: &str) -> Result<Vec<WorkoutSession>> {
        let sessions = sqlx::query_as::<_, WorkoutSession>(
            "SELECT * FROM workout_sessions WHERE user_id = ? ORDER BY date DESC"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(sessions)
    }

    pub async fn find_sessions_by_user_paginated(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<WorkoutSession>> {
        let sessions = sqlx::query_as::<_, WorkoutSession>(
            "SELECT * FROM workout_sessions WHERE user_id = ? ORDER BY date DESC LIMIT ? OFFSET ?"
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(sessions)
    }

    pub async fn count_sessions_by_user(&self, user_id: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM workout_sessions WHERE user_id = ?"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }

    pub async fn update_session(
        &self,
        id: &str,
        user_id: &str,
        date: Option<NaiveDate>,
        notes: Option<&str>,
    ) -> Result<bool> {
        let mut query = String::from("UPDATE workout_sessions SET ");
        let mut updates = Vec::new();

        if date.is_some() {
            updates.push("date = ?");
        }
        updates.push("notes = ?");

        query.push_str(&updates.join(", "));
        query.push_str(" WHERE id = ? AND user_id = ?");

        let mut q = sqlx::query(&query);
        if let Some(d) = date {
            q = q.bind(d);
        }
        q = q.bind(notes).bind(id).bind(user_id);

        let result = q.execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_session(&self, id: &str, user_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM workout_sessions WHERE id = ? AND user_id = ?"
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
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

        sqlx::query(
            "INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight, rpe, is_pr, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)"
        )
        .bind(&id)
        .bind(session_id)
        .bind(exercise_id)
        .bind(set_number)
        .bind(reps)
        .bind(weight)
        .bind(rpe)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(WorkoutLog {
            id,
            session_id: session_id.to_string(),
            exercise_id: exercise_id.to_string(),
            set_number,
            reps,
            weight,
            rpe,
            is_pr: false,
            created_at: now,
        })
    }

    pub async fn find_logs_by_session(&self, session_id: &str) -> Result<Vec<WorkoutLogWithExercise>> {
        let logs = sqlx::query_as::<_, WorkoutLogWithExercise>(
            "SELECT wl.id, wl.session_id, wl.exercise_id, e.name as exercise_name,
                    wl.set_number, wl.reps, wl.weight, wl.rpe, wl.is_pr
             FROM workout_logs wl
             JOIN exercises e ON wl.exercise_id = e.id
             WHERE wl.session_id = ?
             ORDER BY wl.created_at, wl.set_number"
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(logs)
    }

    pub async fn find_log_by_id(&self, id: &str) -> Result<Option<WorkoutLog>> {
        let log = sqlx::query_as::<_, WorkoutLog>(
            "SELECT * FROM workout_logs WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(log)
    }

    pub async fn delete_log(&self, id: &str, session_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM workout_logs WHERE id = ? AND session_id = ?"
        )
        .bind(id)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_as_pr(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("UPDATE workout_logs SET is_pr = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_next_set_number(&self, session_id: &str, exercise_id: &str) -> Result<i32> {
        let result: Option<(i32,)> = sqlx::query_as(
            "SELECT MAX(set_number) FROM workout_logs WHERE session_id = ? AND exercise_id = ?"
        )
        .bind(session_id)
        .bind(exercise_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(n,)| n + 1).unwrap_or(1))
    }

    // Personal Records
    pub async fn find_pr(
        &self,
        user_id: &str,
        exercise_id: &str,
        record_type: &str,
    ) -> Result<Option<PersonalRecord>> {
        let pr = sqlx::query_as::<_, PersonalRecord>(
            "SELECT * FROM personal_records WHERE user_id = ? AND exercise_id = ? AND record_type = ?"
        )
        .bind(user_id)
        .bind(exercise_id)
        .bind(record_type)
        .fetch_optional(&self.pool)
        .await?;
        Ok(pr)
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

        sqlx::query(
            "INSERT INTO personal_records (id, user_id, exercise_id, record_type, value, achieved_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, exercise_id, record_type)
             DO UPDATE SET value = excluded.value, achieved_at = excluded.achieved_at"
        )
        .bind(&id)
        .bind(user_id)
        .bind(exercise_id)
        .bind(record_type)
        .bind(value)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(PersonalRecord {
            id,
            user_id: user_id.to_string(),
            exercise_id: exercise_id.to_string(),
            record_type: record_type.to_string(),
            value,
            achieved_at: now,
        })
    }

    pub async fn find_prs_by_user(&self, user_id: &str) -> Result<Vec<PersonalRecordWithExercise>> {
        let prs = sqlx::query_as::<_, PersonalRecordWithExercise>(
            "SELECT pr.id, pr.user_id, pr.exercise_id, e.name as exercise_name,
                    pr.record_type, pr.value, pr.achieved_at
             FROM personal_records pr
             JOIN exercises e ON pr.exercise_id = e.id
             WHERE pr.user_id = ?
             ORDER BY pr.achieved_at DESC"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(prs)
    }

    pub async fn find_prs_by_exercise(
        &self,
        user_id: &str,
        exercise_id: &str,
    ) -> Result<Vec<PersonalRecord>> {
        let prs = sqlx::query_as::<_, PersonalRecord>(
            "SELECT * FROM personal_records
             WHERE user_id = ? AND exercise_id = ?
             ORDER BY record_type"
        )
        .bind(user_id)
        .bind(exercise_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(prs)
    }

    // Statistics
    pub async fn count_workouts_this_week(&self, user_id: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM workout_sessions
             WHERE user_id = ? AND date >= date('now', '-7 days')"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }

    pub async fn count_workouts_this_month(&self, user_id: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM workout_sessions
             WHERE user_id = ? AND date >= date('now', '-30 days')"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }

    pub async fn get_total_volume_this_week(&self, user_id: &str) -> Result<f64> {
        let result: Option<(f64,)> = sqlx::query_as(
            "SELECT SUM(wl.weight * wl.reps)
             FROM workout_logs wl
             JOIN workout_sessions ws ON wl.session_id = ws.id
             WHERE ws.user_id = ? AND ws.date >= date('now', '-7 days')"
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(result.map(|(v,)| v).unwrap_or(0.0))
    }

    pub async fn get_exercise_history(
        &self,
        user_id: &str,
        exercise_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkoutLog>> {
        let logs = sqlx::query_as::<_, WorkoutLog>(
            "SELECT wl.* FROM workout_logs wl
             JOIN workout_sessions ws ON wl.session_id = ws.id
             WHERE ws.user_id = ? AND wl.exercise_id = ?
             ORDER BY ws.date DESC, wl.set_number
             LIMIT ?"
        )
        .bind(user_id)
        .bind(exercise_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(logs)
    }
}
