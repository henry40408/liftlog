use chrono::NaiveDate;
use rusqlite::Row;
use serde::Serialize;

use super::FromSqliteRow;

/// One row per workout session that contains the queried exercise.
/// Returned by `WorkoutRepository::get_session_metrics_for_exercise`.
#[derive(Debug, Clone)]
pub struct ExerciseSessionMetric {
    pub session_id: String,
    pub date: NaiveDate,
    pub top_weight: f64,
    pub top_reps: i32,
    pub volume: f64,
}

impl FromSqliteRow for ExerciseSessionMetric {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            session_id: row.get("session_id")?,
            date: row.get("date")?,
            top_weight: row.get("top_weight")?,
            top_reps: row.get("top_reps")?,
            volume: row.get("volume")?,
        })
    }
}

/// Chart-ready point. `e1rm` is derived via Epley from `(top_weight, top_reps)`.
/// Serialized into the page as JSON for the client-side switch handler.
#[derive(Debug, Clone, Serialize)]
pub struct ChartPoint {
    pub session_id: String,
    pub date: NaiveDate,
    pub top_weight: f64,
    pub top_reps: i32,
    pub volume: f64,
    pub e1rm: f64,
}

impl ChartPoint {
    pub fn from_metric(m: &ExerciseSessionMetric) -> Self {
        Self {
            session_id: m.session_id.clone(),
            date: m.date,
            top_weight: m.top_weight,
            top_reps: m.top_reps,
            volume: m.volume,
            e1rm: m.top_weight * (1.0 + m.top_reps as f64 / 30.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_point_from_metric_computes_epley_e1rm() {
        let metric = ExerciseSessionMetric {
            session_id: "s1".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            top_weight: 100.0,
            top_reps: 6,
            volume: 600.0,
        };
        let point = ChartPoint::from_metric(&metric);
        // Epley: 100 * (1 + 6/30) = 100 * 1.2 = 120.0
        assert!((point.e1rm - 120.0).abs() < 1e-9);
        assert_eq!(point.session_id, "s1");
        assert_eq!(point.top_weight, 100.0);
        assert_eq!(point.top_reps, 6);
        assert_eq!(point.volume, 600.0);
    }

    #[test]
    fn chart_point_e1rm_for_single_rep_equals_top_weight() {
        let metric = ExerciseSessionMetric {
            session_id: "s2".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
            top_weight: 140.0,
            top_reps: 0,
            volume: 0.0,
        };
        let point = ChartPoint::from_metric(&metric);
        // 1 + 0/30 = 1.0 → e1rm equals top_weight
        assert!((point.e1rm - 140.0).abs() < 1e-9);
    }
}
