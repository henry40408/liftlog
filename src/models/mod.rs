pub mod exercise;
pub mod exercise_session_metric;
pub mod from_row;
pub mod personal_record;
pub mod user;
pub mod workout_log;
pub mod workout_session;

pub use exercise::{CreateExercise, Exercise, UpdateExercise};
pub use exercise_session_metric::{ChartPoint, ExerciseSessionMetric};
pub use from_row::FromSqliteRow;
pub use personal_record::{
    DynamicPR, LastExerciseWeight, PersonalRecordSummary, recent_pr_window_start,
};
// The length bounds themselves are deliberately not re-exported here: every
// in-crate caller goes through `password_length_error` instead, so a second
// place enforcing its own idea of "too short" cannot quietly appear. Tests
// that need the numbers reach for `models::user::{MIN,MAX}_PASSWORD_LEN`.
pub use user::{CreateUser, LoginCredentials, User, UserListItem, UserRole, password_policy_error};
pub use workout_log::{CreateWorkoutLog, UpdateWorkoutLog, WorkoutLog, WorkoutLogWithExercise};
pub use workout_session::{CreateWorkoutSession, WorkoutSession};
