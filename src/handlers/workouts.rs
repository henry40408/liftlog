use askama::Template;
use axum::{
    Form,
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer};

use crate::error::{AppError, Result};
use crate::handlers::confirm;
use crate::middleware::AuthUser;
use crate::models::exercise::{CATEGORIES, ExerciseCategory};
use crate::models::{
    CreateWorkoutLog, CreateWorkoutSession, Exercise, LastExerciseWeight, UpdateWorkoutLog,
    WorkoutLog, WorkoutLogWithExercise, WorkoutSession, recent_pr_window_start,
};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "workouts/list.html")]
struct WorkoutsListTemplate {
    user: AuthUser,
    workouts: Vec<WorkoutSession>,
    page: i64,
    total_pages: i64,
}

#[derive(Template)]
#[template(path = "workouts/new.html")]
struct NewWorkoutTemplate {
    user: AuthUser,
    today: NaiveDate,
    error: Option<String>,
}

/// Values the Add Set form starts with when `?prefill=<log_id>` names a set
/// in this workout — the scripts-off half of the Clone button.
pub(crate) struct PrefillSet {
    pub(crate) exercise_id: String,
    pub(crate) weight: f64,
    pub(crate) reps: i32,
    pub(crate) rpe: Option<i32>,
}

/// One row of the scripts-off "last weights" list. With scripts on, the same
/// figures appear inline as the exercise `<select>` changes; nothing can
/// react to that select without them, so the whole set is listed instead.
pub(crate) struct LastWeightRow {
    pub(crate) exercise_name: String,
    pub(crate) weight: f64,
    pub(crate) rpe: Option<i32>,
    pub(crate) logged_at: DateTime<Utc>,
}

/// `?prefill=` on the workout page.
#[derive(Deserialize)]
pub struct ShowQuery {
    prefill: Option<String>,
}

#[derive(Template)]
#[template(path = "workouts/show.html")]
struct ShowWorkoutTemplate {
    user: AuthUser,
    workout: WorkoutSession,
    logs: Vec<WorkoutLogWithExercise>,
    exercises: Vec<Exercise>,
    categories: &'static [ExerciseCategory],
    exercise_last_weights: Vec<LastExerciseWeight>,
    /// Same figures as `exercise_last_weights`, joined to exercise names and
    /// sorted, for the `<noscript>` list.
    last_weight_rows: Vec<LastWeightRow>,
    prefill: Option<PrefillSet>,
    share_url: Option<String>,
    share_expires_at: Option<DateTime<Utc>>,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "workouts/shared.html")]
struct SharedWorkoutTemplate {
    workout: WorkoutSession,
    logs: Vec<WorkoutLogWithExercise>,
    owner_username: String,
}

#[derive(Template)]
#[template(path = "workouts/edit.html")]
struct EditWorkoutTemplate {
    user: AuthUser,
    workout: WorkoutSession,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "workouts/edit_log.html")]
struct EditLogTemplate {
    user: AuthUser,
    workout: WorkoutSession,
    log: WorkoutLog,
    exercise_name: String,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    page: Option<i64>,
}

pub async fn list(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = 10;
    let offset = (page - 1) * per_page;

    let workouts = state
        .workout_repo
        .find_sessions_by_user_paginated(&auth_user.id, per_page, offset)
        .await?;

    let total = state
        .workout_repo
        .count_sessions_by_user(&auth_user.id)
        .await?;
    let total_pages = (total + per_page - 1) / per_page;

    let template = WorkoutsListTemplate {
        user: auth_user,
        workouts,
        page,
        total_pages,
    };

    Ok(Html(template.render()?).into_response())
}

pub async fn new_page(auth_user: AuthUser) -> Result<Response> {
    let today = chrono::Local::now().date_naive();

    let template = NewWorkoutTemplate {
        user: auth_user,
        today,
        error: None,
    };

    Ok(Html(template.render()?).into_response())
}

pub async fn create(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Form(form): Form<CreateWorkoutSession>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .create_session(&auth_user.id, form.date, form.notes.as_deref())
        .await?;

    Ok(Redirect::to(&format!("/workouts/{}", workout.id)).into_response())
}

pub async fn show(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<ShowQuery>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .find_owned_session(&id, &auth_user.id)
        .await?;

    let logs = state
        .workout_repo
        .find_logs_by_session_with_pr(&id, &auth_user.id, recent_pr_window_start())
        .await?;
    let exercises = state
        .exercise_repo
        .find_available_for_user(&auth_user.id)
        .await?;
    let exercise_last_weights = state
        .workout_repo
        .get_last_weight_per_exercise_by_user(&auth_user.id)
        .await?;

    // Resolved against this session's own logs, which are already scoped to
    // the caller — so a `prefill` id from someone else's workout simply
    // fails to match rather than disclosing anything.
    let prefill = query.prefill.as_ref().and_then(|log_id| {
        logs.iter().find(|l| &l.id == log_id).map(|l| PrefillSet {
            exercise_id: l.exercise_id.clone(),
            weight: l.weight,
            reps: l.reps,
            rpe: l.rpe,
        })
    });

    let mut last_weight_rows: Vec<LastWeightRow> = exercises
        .iter()
        .filter_map(|ex| {
            exercise_last_weights
                .iter()
                .find(|w| w.exercise_id == ex.id)
                .map(|w| LastWeightRow {
                    exercise_name: ex.name.clone(),
                    weight: w.weight,
                    rpe: w.rpe,
                    logged_at: w.logged_at,
                })
        })
        .collect();
    // `exercises` is ordered by category then name; a flat alphabetical list
    // is easier to scan when the point is looking one exercise up.
    last_weight_rows.sort_by(|a, b| a.exercise_name.cmp(&b.exercise_name));

    let share_url = workout
        .share_token
        .as_ref()
        .map(|token| format!("/shared/{token}"));
    let share_expires_at = workout.share_expires_at;

    let template = ShowWorkoutTemplate {
        user: auth_user,
        workout,
        logs,
        exercises,
        categories: CATEGORIES,
        exercise_last_weights,
        last_weight_rows,
        prefill,
        share_url,
        share_expires_at,
        error: None,
    };

    Ok(Html(template.render()?).into_response())
}

pub async fn edit_page(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .find_owned_session(&id, &auth_user.id)
        .await?;

    let template = EditWorkoutTemplate {
        user: auth_user,
        workout,
        error: None,
    };

    Ok(Html(template.render()?).into_response())
}

#[derive(Deserialize)]
pub struct UpdateWorkoutForm {
    pub date: NaiveDate,
    pub notes: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Form(form): Form<UpdateWorkoutForm>,
) -> Result<Response> {
    state
        .workout_repo
        .update_session(&id, &auth_user.id, Some(form.date), form.notes.as_deref())
        .await?;

    Ok(Redirect::to(&format!("/workouts/{id}")).into_response())
}

/// Interstitial for `delete`. Counts the sets first: deleting a session
/// cascades to its `workout_logs` rows (migrations/004), and that is the part
/// worth spelling out before the click.
pub async fn confirm_delete(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .find_owned_session(&id, &auth_user.id)
        .await?;
    let logs = state
        .workout_repo
        .find_logs_by_session_with_pr(&id, &auth_user.id, recent_pr_window_start())
        .await?;

    let sets = match logs.len() {
        0 => "It has no sets recorded.".to_string(),
        1 => "Its 1 recorded set will be deleted with it.".to_string(),
        n => format!("All {n} of its recorded sets will be deleted with it."),
    };

    confirm::page(
        auth_user,
        "Delete workout",
        format!(
            "The workout on {date} will be permanently deleted. {sets} This cannot be undone.",
            date = workout.date
        ),
        format!("/workouts/{id}/delete"),
        format!("/workouts/{id}"),
    )
}

pub async fn delete(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    state
        .workout_repo
        .delete_session(&id, &auth_user.id)
        .await?;
    Ok(Redirect::to("/workouts").into_response())
}

pub async fn add_log(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(session_id): Path<String>,
    Form(form): Form<CreateWorkoutLog>,
) -> Result<Response> {
    state
        .workout_repo
        .find_owned_session(&session_id, &auth_user.id)
        .await?;

    // `exercise_id` arrives from the form body, so owning the session is not
    // enough — without this a caller could attach a log to another user's
    // exercise, which the UI's own <select> would never offer.
    state
        .exercise_repo
        .find_owned(&form.exercise_id, &auth_user.id)
        .await?;

    let set_number = state
        .workout_repo
        .get_next_set_number(&session_id, &form.exercise_id)
        .await?;

    state
        .workout_repo
        .create_log(
            &session_id,
            &form.exercise_id,
            set_number,
            form.reps,
            form.weight,
            form.rpe,
        )
        .await?;

    Ok(Redirect::to(&format!("/workouts/{session_id}")).into_response())
}

/// Interstitial for `delete_log`. Names the set being removed by pulling it
/// out of the session's own logs, which also proves it belongs to that
/// session — a log id from someone else's workout is a 404 here, exactly as
/// it is in `delete_log`.
pub async fn confirm_delete_log(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((session_id, log_id)): Path<(String, String)>,
) -> Result<Response> {
    state
        .workout_repo
        .find_owned_session(&session_id, &auth_user.id)
        .await?;

    let log = state
        .workout_repo
        .find_logs_by_session_with_pr(&session_id, &auth_user.id, recent_pr_window_start())
        .await?
        .into_iter()
        .find(|l| l.id == log_id)
        .ok_or_else(|| AppError::NotFound("Set not found".to_string()))?;

    confirm::page(
        auth_user,
        "Delete set",
        format!(
            "Set {set} of {exercise} — {weight} kg × {reps} — will be permanently deleted.",
            set = log.set_number,
            exercise = log.exercise_name,
            weight = log.weight,
            reps = log.reps
        ),
        format!("/workouts/{session_id}/logs/{log_id}/delete"),
        format!("/workouts/{session_id}"),
    )
}

pub async fn delete_log(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((session_id, log_id)): Path<(String, String)>,
) -> Result<Response> {
    state
        .workout_repo
        .find_owned_session(&session_id, &auth_user.id)
        .await?;

    state.workout_repo.delete_log(&log_id, &session_id).await?;

    Ok(Redirect::to(&format!("/workouts/{session_id}")).into_response())
}

pub async fn edit_log_page(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((session_id, log_id)): Path<(String, String)>,
) -> Result<Response> {
    let session = state
        .workout_repo
        .find_owned_session(&session_id, &auth_user.id)
        .await?;

    let log = state
        .workout_repo
        .find_log_by_id(&log_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Log not found".to_string()))?;

    if log.session_id != session_id {
        return Err(AppError::NotFound("Log not found".to_string()));
    }

    let exercise = state
        .exercise_repo
        .find_by_id(&log.exercise_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Exercise not found".to_string()))?;

    let template = EditLogTemplate {
        user: auth_user,
        workout: session,
        log,
        exercise_name: exercise.name,
        error: None,
    };

    Ok(Html(template.render()?).into_response())
}

pub async fn update_log(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((session_id, log_id)): Path<(String, String)>,
    Form(form): Form<UpdateWorkoutLog>,
) -> Result<Response> {
    state
        .workout_repo
        .find_owned_session(&session_id, &auth_user.id)
        .await?;

    state
        .workout_repo
        .update_log(&log_id, &session_id, form.reps, form.weight, form.rpe)
        .await?;

    Ok(Redirect::to(&format!("/workouts/{session_id}")).into_response())
}

/// Deserialize an optional TTL (in days) from a form field. An absent field
/// or an empty/whitespace-only string — what the "Never expires" `<select>`
/// option submits — means `None`; anything else must parse as an integer.
/// `Option<String>::deserialize` (rather than requiring a `String`) is what
/// makes the absent-field case work at all: axum's form deserializer never
/// invokes this function for a missing key unless the target type itself
/// tolerates absence.
fn empty_string_as_none<'de, D>(deserializer: D) -> std::result::Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => s.trim().parse().map(Some).map_err(serde::de::Error::custom),
    }
}

#[derive(Deserialize)]
pub struct ShareForm {
    /// Days the share link stays valid. `None` — the field absent, or an
    /// empty string from the "never expires" <select> option — means never
    /// expires, preserving pre-012 behaviour.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub expires_in_days: Option<i64>,
}

pub async fn share_workout(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Form(form): Form<ShareForm>,
) -> Result<Response> {
    if let Some(days) = form.expires_in_days
        && !(1..=365).contains(&days)
    {
        return Err(AppError::BadRequest(
            "Share link expiry must be between 1 and 365 days".to_string(),
        ));
    }

    state
        .workout_repo
        .find_owned_session(&id, &auth_user.id)
        .await?;

    state
        .workout_repo
        .set_share_token(
            &id,
            &auth_user.id,
            form.expires_in_days.map(chrono::Duration::days),
        )
        .await?;

    Ok(Redirect::to(&format!("/workouts/{id}")).into_response())
}

/// Interstitial for `revoke_share`. Revoking drops the token, so the link
/// already handed out stops working for everyone at once and a fresh share
/// produces a different URL — not obvious from a button labelled "Revoke".
pub async fn confirm_revoke_share(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    state
        .workout_repo
        .find_owned_session(&id, &auth_user.id)
        .await?;

    confirm::page(
        auth_user,
        "Revoke share link",
        "The existing share link will stop working for anyone who already has it. \
         Sharing this workout again will produce a different link."
            .to_string(),
        format!("/workouts/{id}/revoke-share"),
        format!("/workouts/{id}"),
    )
}

pub async fn revoke_share(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    state
        .workout_repo
        .find_owned_session(&id, &auth_user.id)
        .await?;

    state
        .workout_repo
        .revoke_share_token(&id, &auth_user.id)
        .await?;

    Ok(Redirect::to(&format!("/workouts/{id}")).into_response())
}

pub async fn view_shared(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .find_session_by_share_token(&token)
        .await?
        .ok_or_else(|| AppError::NotFound("Shared workout not found".to_string()))?;

    let logs = state
        .workout_repo
        .find_logs_by_session_for_share(&workout.id)
        .await?;

    let owner = state
        .user_repo
        .find_by_id(&workout.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let template = SharedWorkoutTemplate {
        workout,
        logs,
        owner_username: owner.username,
    };

    Ok(Html(template.render()?).into_response())
}

#[cfg(test)]
mod tests {
    use super::ShareForm;

    // `Option<String>::deserialize` behaves identically regardless of the
    // wire format feeding it a field value, so exercising it through
    // `serde_json` (already a direct dependency) covers the same code path
    // `axum::Form`'s urlencoded deserializer would, without adding a new
    // dependency just for these tests.
    fn parse(json: &str) -> std::result::Result<ShareForm, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn absent_field_means_never_expires() {
        let form = parse("{}").unwrap();
        assert_eq!(form.expires_in_days, None);
    }

    #[test]
    fn empty_string_means_never_expires() {
        let form = parse(r#"{"expires_in_days": ""}"#).unwrap();
        assert_eq!(form.expires_in_days, None);
    }

    #[test]
    fn whitespace_only_string_means_never_expires() {
        let form = parse(r#"{"expires_in_days": "   "}"#).unwrap();
        assert_eq!(form.expires_in_days, None);
    }

    #[test]
    fn numeric_string_parses_to_some() {
        let form = parse(r#"{"expires_in_days": "7"}"#).unwrap();
        assert_eq!(form.expires_in_days, Some(7));
    }

    #[test]
    fn non_numeric_string_is_a_deserialization_error() {
        assert!(parse(r#"{"expires_in_days": "abc"}"#).is_err());
    }
}
