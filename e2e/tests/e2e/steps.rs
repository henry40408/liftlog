//! Step definitions, ported from `tests/e2e/steps/*.js`.
//!
//! They live in the test binary rather than the library because the `#[given]`
//! / `#[when]` / `#[then]` macros register through `inventory`, and a step that
//! is only reachable through an rlib can be dropped by the linker.
//!
//! Where the old steps wrote `await expect(...)`, these call [`eventually`] or
//! [`eventually_eq`]: `WebDriver` has no retrying-assertion layer, and most of
//! these assertions land immediately after a form post that is still in flight.

use anyhow::{Result, ensure};
use cucumber::{given, then, when};

use liftlog_e2e::http;
use liftlog_e2e::pages::goto;
use liftlog_e2e::seeding::{ensure_user, open_second_session};
use liftlog_e2e::server::{ADMIN, PASSWORD};
use liftlog_e2e::wait::{eventually, eventually_eq};
use liftlog_e2e::world::LiftLogWorld;

// --- common --------------------------------------------------------------

#[when(expr = "I visit {string}")]
async fn visit(world: &mut LiftLogWorld, path: String) -> Result<()> {
    goto(world.driver()?, &path).await
}

#[then(expr = "the URL is {string}")]
async fn url_is(world: &mut LiftLogWorld, path: String) -> Result<()> {
    eventually_eq("the URL", path.as_str(), || async { world.path().await }).await
}

// --- authentication ------------------------------------------------------

#[given(expr = "a user {string} with password {string} exists")]
async fn user_exists(world: &mut LiftLogWorld, username: String, password: String) -> Result<()> {
    let _ = world;
    ensure_user(&username, &password).await
}

#[given(expr = "I am logged in as {string}")]
async fn logged_in_as(world: &mut LiftLogWorld, username: String) -> Result<()> {
    sign_in(world, &username, PASSWORD).await
}

#[given(expr = "I am logged in as {string} with password {string}")]
async fn logged_in_as_with_password(
    world: &mut LiftLogWorld,
    username: String,
    password: String,
) -> Result<()> {
    sign_in(world, &username, &password).await
}

#[when(expr = "I log in as {string} with password {string}")]
async fn log_in(world: &mut LiftLogWorld, username: String, password: String) -> Result<()> {
    world.login_page()?.login(&username, &password).await
}

#[when("I log out")]
async fn log_out(world: &mut LiftLogWorld) -> Result<()> {
    world.nav()?.sign_out().await
}

#[given("I am logged in as a fresh non-admin user")]
async fn logged_in_as_fresh_user(world: &mut LiftLogWorld) -> Result<()> {
    let username = world.unique("member");
    ensure_user(&username, PASSWORD).await?;
    sign_in(world, &username, PASSWORD).await
}

#[when("I switch to a fresh non-admin user")]
async fn switch_to_fresh_user(world: &mut LiftLogWorld) -> Result<()> {
    world.browser()?.clear_cookies().await?;
    let username = world.unique("other");
    ensure_user(&username, PASSWORD).await?;
    sign_in(world, &username, PASSWORD).await
}

#[then("I see the dashboard")]
async fn see_dashboard(world: &mut LiftLogWorld) -> Result<()> {
    eventually_eq("the URL", "/", || async { world.path().await }).await?;
    eventually("the dashboard heading is showing", || async {
        world.dashboard()?.is_showing().await
    })
    .await
}

#[then("I see the login page")]
async fn see_login_page(world: &mut LiftLogWorld) -> Result<()> {
    eventually_eq("the URL", "/auth/login", || async { world.path().await }).await?;
    eventually("the login form is showing", || async {
        world.login_page()?.is_showing().await
    })
    .await
}

#[then("I see the setup page")]
async fn see_setup_page(world: &mut LiftLogWorld) -> Result<()> {
    eventually_eq("the URL", "/auth/setup", || async { world.path().await }).await?;
    eventually("the setup form is showing", || async {
        world.setup_page()?.is_showing().await
    })
    .await
}

#[then(expr = "I see the login error {string}")]
async fn see_login_error(world: &mut LiftLogWorld, message: String) -> Result<()> {
    eventually(&format!("the login error mentions `{message}`"), || async {
        Ok(contains(world.login_page()?.error().await?, &message))
    })
    .await
}

#[when(expr = "I submit the setup form with username {string} and password {string}")]
async fn submit_setup(world: &mut LiftLogWorld, username: String, password: String) -> Result<()> {
    world.setup_page()?.submit(&username, &password).await
}

#[then(expr = "I see the setup error {string}")]
async fn see_setup_error(world: &mut LiftLogWorld, message: String) -> Result<()> {
    eventually_eq("the URL", "/auth/setup", || async { world.path().await }).await?;
    eventually(&format!("the setup error mentions `{message}`"), || async {
        Ok(contains(world.setup_page()?.error().await?, &message))
    })
    .await
}

// --- dashboard -----------------------------------------------------------

#[then("the dashboard lists the workout I created in Recent Workouts")]
async fn dashboard_lists_workout(world: &mut LiftLogWorld) -> Result<()> {
    world.dashboard()?.goto().await?;
    eventually("the Recent Workouts heading is showing", || async {
        world.dashboard()?.has_recent_heading().await
    })
    .await?;
    eventually("the workout is on the dashboard", || async {
        world.dashboard()?.lists_workout(world.workout_id()?).await
    })
    .await
}

#[then("the dashboard actions sit above the summary and the workout list")]
async fn dashboard_actions_lead(world: &mut LiftLogWorld) -> Result<()> {
    world.dashboard()?.goto().await?;
    eventually("the quick actions render first", || async {
        world.dashboard()?.actions_lead().await
    })
    .await
}

#[then(expr = "the dashboard {string} count is {int}")]
async fn dashboard_count(world: &mut LiftLogWorld, label: String, count: i64) -> Result<()> {
    world.dashboard()?.goto().await?;
    eventually_eq(
        &format!("the `{label}` card"),
        count.to_string().as_str(),
        || async { world.dashboard()?.stat(&label).await },
    )
    .await
}

// --- exercises -----------------------------------------------------------

#[when(expr = "I create a new exercise in category {string}")]
async fn create_exercise(world: &mut LiftLogWorld, category: String) -> Result<()> {
    let name = world.unique("Squat");
    add_exercise(world, &name, &category).await
}

#[given(expr = "I have an exercise in category {string}")]
async fn have_exercise(world: &mut LiftLogWorld, category: String) -> Result<()> {
    let name = world.unique("Exercise");
    add_exercise(world, &name, &category).await
}

#[then("the exercise I created is listed on the exercises page")]
async fn exercise_is_listed(world: &mut LiftLogWorld) -> Result<()> {
    world.exercises_page()?.goto().await?;
    eventually_eq("entries naming my exercise", 1usize, || async {
        world
            .exercises_page()?
            .entries_named(world.exercise()?)
            .await
    })
    .await
}

#[when("I rename my exercise")]
async fn rename_exercise(world: &mut LiftLogWorld) -> Result<()> {
    let renamed = world.unique("Renamed");
    world
        .exercises_page()?
        .rename(world.exercise()?, &renamed)
        .await?;
    eventually_eq("the URL", "/exercises", || async { world.path().await }).await?;
    world.exercise_name = Some(renamed);
    Ok(())
}

#[when("I delete my exercise")]
async fn delete_exercise(world: &mut LiftLogWorld) -> Result<()> {
    world.exercises_page()?.delete(world.exercise()?).await?;
    eventually_eq("the URL", "/exercises", || async { world.path().await }).await
}

#[then("my exercise is no longer listed on the exercises page")]
async fn exercise_is_gone(world: &mut LiftLogWorld) -> Result<()> {
    world.exercises_page()?.goto().await?;
    eventually_eq("entries naming my exercise", 0usize, || async {
        world
            .exercises_page()?
            .entries_named(world.exercise()?)
            .await
    })
    .await
}

// --- workouts ------------------------------------------------------------

#[when("I start a new workout for today")]
async fn start_workout(world: &mut LiftLogWorld) -> Result<()> {
    create_workout(world).await
}

#[given("I have a workout")]
async fn have_workout(world: &mut LiftLogWorld) -> Result<()> {
    create_workout(world).await
}

#[given(expr = "I have a workout with a set of {int} kg for {int} reps")]
async fn have_workout_with_set(world: &mut LiftLogWorld, weight: i64, reps: i64) -> Result<()> {
    ensure!(
        world.exercise_name.is_some(),
        "this step requires an exercise to exist first"
    );
    create_workout(world).await?;
    let exercise = world.exercise()?.to_string();
    world
        .workout()?
        .log_set(&exercise, &weight.to_string(), &reps.to_string(), None)
        .await
}

#[then(expr = "the {word} field suggests {string}")]
async fn field_suggests(world: &mut LiftLogWorld, field: String, value: String) -> Result<()> {
    let offered = world.workout()?.suggestions(&field).await?;
    ensure!(
        offered.contains(&value),
        "the `{field}` field offers {offered:?}, which does not include {value:?}"
    );
    Ok(())
}

#[when(expr = "I log a set of {int} kg for {int} reps using the exercise I created")]
async fn log_set(world: &mut LiftLogWorld, weight: i64, reps: i64) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    let workout = world.workout()?;
    workout.goto().await?;
    workout
        .log_set(&exercise, &weight.to_string(), &reps.to_string(), None)
        .await
}

#[when(expr = "I log a set of {int} kg for {int} reps with RPE {int} using the exercise I created")]
async fn log_set_with_rpe(
    world: &mut LiftLogWorld,
    weight: i64,
    reps: i64,
    rpe: i64,
) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    let workout = world.workout()?;
    workout.goto().await?;
    workout
        .log_set(
            &exercise,
            &weight.to_string(),
            &reps.to_string(),
            Some(&rpe.to_string()),
        )
        .await
}

#[when(expr = "I log another set of {int} kg for {int} reps using the same exercise")]
async fn log_another_set(world: &mut LiftLogWorld, weight: i64, reps: i64) -> Result<()> {
    log_set(world, weight, reps).await
}

#[when(expr = "I edit my set to {int} kg for {int} reps")]
async fn edit_set(world: &mut LiftLogWorld, weight: i64, reps: i64) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    let workout = world.workout()?;
    workout.goto().await?;
    workout.open_edit_set(&exercise).await?;
    world
        .edit_log_page()?
        .save(&weight.to_string(), &reps.to_string())
        .await?;
    on_the_workout(world).await
}

#[when("I delete my set")]
async fn delete_set(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    let workout = world.workout()?;
    workout.goto().await?;
    workout.delete_set(&exercise).await?;
    on_the_workout(world).await
}

#[when("I delete the workout")]
async fn delete_workout(world: &mut LiftLogWorld) -> Result<()> {
    let workout = world.workout()?;
    workout.goto().await?;
    workout.delete().await?;
    eventually_eq("the URL", "/workouts", || async { world.path().await }).await
}

#[when(expr = "I edit the workout to date {string} with notes {string}")]
async fn edit_workout(world: &mut LiftLogWorld, date: String, notes: String) -> Result<()> {
    world.edit_workout()?.save(&date, &notes).await?;
    on_the_workout(world).await
}

#[when("I select the exercise I created on the workout page")]
async fn select_exercise(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    let workout = world.workout()?;
    workout.goto().await?;
    workout.select_exercise(&exercise).await
}

#[when("I click clone on my set")]
async fn clone_set(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    let workout = world.workout()?;
    workout.goto().await?;
    workout.clone_set(&exercise).await
}

#[then("I am on the workout detail page")]
async fn am_on_workout_page(world: &mut LiftLogWorld) -> Result<()> {
    on_the_workout(world).await
}

#[then("the workout I created is listed on the workouts page")]
async fn workout_is_listed(world: &mut LiftLogWorld) -> Result<()> {
    world.workouts_page()?.goto().await?;
    eventually_eq("links to my workout", 1usize, || async {
        world.workouts_page()?.links_to(world.workout_id()?).await
    })
    .await
}

#[then("the workout I deleted is not listed on the workouts page")]
async fn deleted_workout_is_gone(world: &mut LiftLogWorld) -> Result<()> {
    workout_is_not_listed(world).await
}

#[then("I do not see the workout I created on the workouts page")]
async fn workout_is_not_listed(world: &mut LiftLogWorld) -> Result<()> {
    world.workouts_page()?.goto().await?;
    eventually_eq("links to my workout", 0usize, || async {
        world.workouts_page()?.links_to(world.workout_id()?).await
    })
    .await
}

#[then(expr = "I see my set logged at {int} kg for {int} reps")]
async fn see_set_logged(world: &mut LiftLogWorld, weight: i64, reps: i64) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world.workout()?.goto().await?;
    eventually_eq("the weight cell", weight.to_string().as_str(), || async {
        world.workout()?.cell(&exercise, "weight").await
    })
    .await?;
    eventually_eq("the reps cell", reps.to_string().as_str(), || async {
        world.workout()?.cell(&exercise, "reps").await
    })
    .await
}

#[then(expr = "I see my set logged with RPE {int}")]
async fn see_set_rpe(world: &mut LiftLogWorld, rpe: i64) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world.workout()?.goto().await?;
    eventually_eq("the RPE cell", rpe.to_string().as_str(), || async {
        world.workout()?.cell(&exercise, "rpe").await
    })
    .await
}

#[then("my set is no longer shown on the workout")]
async fn set_is_gone(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world.workout()?.goto().await?;
    eventually_eq("set rows for my exercise", 0usize, || async {
        Ok(world.workout()?.rows(&exercise).await?.len())
    })
    .await
}

#[then(expr = "the workout detail shows date {string} and notes {string}")]
async fn workout_shows_date_and_notes(
    world: &mut LiftLogWorld,
    date: String,
    notes: String,
) -> Result<()> {
    world.workout()?.goto().await?;
    eventually_eq("the workout heading", date.as_str(), || async {
        world.workout()?.heading().await
    })
    .await?;
    eventually_eq("the workout notes", Some(notes.clone()), || async {
        world.workout()?.notes().await
    })
    .await
}

#[then("my set is flagged as a PR")]
async fn set_is_a_pr(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world.workout()?.goto().await?;
    // A first-ever set is both the all-time and the 1-month best; the all-time
    // badge wins, so the row shows "PR" rather than "PR 1M".
    eventually_eq("the PR badge", Some("PR".to_string()), || async {
        world.workout()?.pr_badge(&exercise).await
    })
    .await
}

#[then("I see two sets numbered 1 and 2")]
async fn two_numbered_sets(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world.workout()?.goto().await?;
    eventually_eq(
        "the set numbers",
        vec!["1".to_string(), "2".to_string()],
        || async { world.workout()?.set_numbers(&exercise).await },
    )
    .await
}

#[then(expr = "the Last info shows {string}")]
async fn last_info_shows(world: &mut LiftLogWorld, text: String) -> Result<()> {
    eventually_eq("the Last info RPE chip", Some(text.clone()), || async {
        world.workout()?.last_info_rpe().await
    })
    .await
}

#[then(expr = "the Add Set form is pre-filled with weight {int} and reps {int}")]
async fn add_set_form_prefilled(world: &mut LiftLogWorld, weight: i64, reps: i64) -> Result<()> {
    let exercise_id = world.exercise_id()?.to_string();
    eventually_eq("the selected exercise", exercise_id.as_str(), || async {
        world.workout()?.prefilled_exercise().await
    })
    .await?;
    eventually_eq("the weight field", weight.to_string().as_str(), || async {
        world.workout()?.prefilled_weight().await
    })
    .await?;
    eventually_eq("the reps field", reps.to_string().as_str(), || async {
        world.workout()?.prefilled_reps().await
    })
    .await
}

#[then("I see the workouts empty state")]
async fn see_empty_state(world: &mut LiftLogWorld) -> Result<()> {
    world.workouts_page()?.goto().await?;
    eventually("the empty state mentions having no workouts", || async {
        Ok(contains(
            world.workouts_page()?.empty_state().await?,
            "No workouts yet",
        ))
    })
    .await
}

#[then("visiting the workout I created returns a 404")]
async fn my_workout_is_404(world: &mut LiftLogWorld) -> Result<()> {
    let path = format!("/workouts/{}", world.workout_id()?);
    let status = world.status_of(&path).await?;
    ensure!(status == 404, "{path} answered {status}, not 404");
    Ok(())
}

#[then(expr = "visiting {string} returns a 404")]
async fn path_is_404(world: &mut LiftLogWorld, path: String) -> Result<()> {
    let status = world.status_of(&path).await?;
    ensure!(status == 404, "{path} answered {status}, not 404");
    Ok(())
}

#[then(expr = "visiting {string} returns a 403")]
async fn path_is_403(world: &mut LiftLogWorld, path: String) -> Result<()> {
    let status = world.status_of(&path).await?;
    ensure!(status == 403, "{path} answered {status}, not 403");
    Ok(())
}

// --- settings ------------------------------------------------------------

#[when(expr = "I change my password from {string} to {string}")]
async fn change_password(world: &mut LiftLogWorld, current: String, next: String) -> Result<()> {
    world
        .settings_page()?
        .change_password(&current, &next, &next)
        .await
}

#[when(expr = "I submit the password form with current {string}, new {string}, confirm {string}")]
async fn submit_password_form(
    world: &mut LiftLogWorld,
    current: String,
    next: String,
    confirm: String,
) -> Result<()> {
    world
        .settings_page()?
        .change_password(&current, &next, &confirm)
        .await
}

#[then("I see a password-change success message")]
async fn see_password_success(world: &mut LiftLogWorld) -> Result<()> {
    eventually("the success banner is showing", || async {
        Ok(contains(
            world.settings_page()?.success().await?,
            "Password changed successfully",
        ))
    })
    .await
}

#[then(expr = "I see a settings error {string}")]
async fn see_settings_error(world: &mut LiftLogWorld, message: String) -> Result<()> {
    eventually(
        &format!("the settings error mentions `{message}`"),
        || async { Ok(contains(world.settings_page()?.error().await?, &message)) },
    )
    .await
}

#[given(expr = "I have a second session as {string}")]
async fn second_session(world: &mut LiftLogWorld, username: String) -> Result<()> {
    let _ = world;
    open_second_session(&username, PASSWORD).await
}

#[when("I log out all other devices")]
async fn log_out_others(world: &mut LiftLogWorld) -> Result<()> {
    world.settings_page()?.log_out_other_devices().await?;
    // The POST re-renders /settings in place rather than redirecting, so the
    // banner is what says it landed.
    eventually("the log-out-others banner is showing", || async {
        Ok(contains(
            world.settings_page()?.success().await?,
            "Logged out of all other devices.",
        ))
    })
    .await
}

#[then(expr = "the active sessions table has {int} row(s)")]
async fn sessions_row_count(world: &mut LiftLogWorld, count: i64) -> Result<()> {
    let expected = usize::try_from(count)?;
    world.settings_page()?.goto().await?;
    eventually_eq("the active sessions row count", expected, || async {
        world.settings_page()?.session_count().await
    })
    .await
}

#[then("the active sessions table marks my current device")]
async fn sessions_mark_current(world: &mut LiftLogWorld) -> Result<()> {
    world.settings_page()?.goto().await?;
    eventually("a row is marked as this device", || async {
        world.settings_page()?.marks_current_device().await
    })
    .await
}

#[then("the active sessions table labels every cell")]
async fn sessions_label_cells(world: &mut LiftLogWorld) -> Result<()> {
    world.settings_page()?.goto().await?;
    let expected: Vec<String> = ["Device", "Last active", "Signed in"]
        .iter()
        .map(ToString::to_string)
        .collect();
    eventually_eq("the session cell labels", expected, || async {
        world.settings_page()?.first_row_labels().await
    })
    .await
}

#[then(
    expr = "the settings page in timezone {string} shows session times ending with {string} for {string} with password {string}"
)]
async fn sessions_in_timezone(
    world: &mut LiftLogWorld,
    timezone: String,
    suffix: String,
    username: String,
    password: String,
) -> Result<()> {
    ensure_user(&username, &password).await?;
    // CDP can retime a live session, so this needs none of the fresh browser
    // context the Playwright step opened to pin `timezoneId`.
    world.browser()?.set_timezone(&timezone).await?;
    world.login_page()?.login(&username, &password).await?;
    see_dashboard(world).await?;
    world.settings_page()?.goto().await?;

    eventually_eq(
        "the number of timestamps in the first row",
        2usize,
        || async { Ok(world.settings_page()?.first_row_times().await?.len()) },
    )
    .await?;

    // `base.html` rewrites each `<time>` into a fixed `YYYY-MM-DD HH:MM GMT±H`,
    // deliberately not `toLocaleString()`, so the shape is exact rather than
    // locale-dependent.
    eventually(
        &format!("both timestamps are rendered in {timezone}"),
        || async {
            let times = world.settings_page()?.first_row_times().await?;
            Ok(!times.is_empty() && times.iter().all(|time| is_local_timestamp(time, &suffix)))
        },
    )
    .await
}

// --- sharing -------------------------------------------------------------

#[when("I share the workout")]
async fn share_workout(world: &mut LiftLogWorld) -> Result<()> {
    publish_share(world).await
}

#[given("I have shared the workout")]
async fn have_shared_workout(world: &mut LiftLogWorld) -> Result<()> {
    publish_share(world).await
}

#[when("I revoke the share")]
async fn revoke_share(world: &mut LiftLogWorld) -> Result<()> {
    let workout = world.workout()?;
    workout.goto().await?;
    workout.revoke_share().await?;
    on_the_workout(world).await?;
    eventually("the share block is gone", || async {
        Ok(!world.workout()?.has_share_info().await?)
    })
    .await
}

#[then("a public share link is shown on the workout page")]
async fn share_link_is_shown(world: &mut LiftLogWorld) -> Result<()> {
    world.workout()?.goto().await?;
    eventually("a share link is on the page", || async {
        Ok(world.workout()?.share_url().await?.is_some())
    })
    .await?;

    let share = world.share_url()?;
    ensure!(
        share.starts_with("/shared/")
            && share.len() > "/shared/".len()
            && share["/shared/".len()..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "`{share}` is not a share path"
    );
    Ok(())
}

// The guest assertions go over plain HTTP rather than through a second browser
// context. The shared page is server-rendered with no scripts of its own, so
// the response body is the whole of what a visitor's browser would show — and
// unlike a `WebDriver` navigation it also carries the status code the revoked
// case is entirely about.
#[then("a guest can view the workout via the share URL")]
async fn guest_can_view_share(world: &mut LiftLogWorld) -> Result<()> {
    let response = http::get(world.share_url()?, None).await?;
    ensure!(
        response.status == 200,
        "a guest got {} from the share URL",
        response.status
    );
    ensure!(
        response.body.contains("Shared by"),
        "the shared page does not name who shared it"
    );
    ensure!(
        response.body.contains("set-row"),
        "the shared page shows no sets"
    );
    Ok(())
}

#[then("a guest visiting the share URL gets a 404")]
async fn guest_gets_404(world: &mut LiftLogWorld) -> Result<()> {
    let status = http::status(world.share_url()?, None).await?;
    ensure!(
        status == 404,
        "a guest got {status} from the revoked share URL"
    );
    Ok(())
}

// --- stats ---------------------------------------------------------------

#[then("I see the stats overview")]
async fn see_stats_overview(world: &mut LiftLogWorld) -> Result<()> {
    world.stats_page()?.goto_overview().await?;
    eventually("the stats overview is showing", || async {
        world.stats_page()?.overview_is_showing().await
    })
    .await
}

#[then("I see exercise-specific stats for the exercise I created")]
async fn see_exercise_stats(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world
        .stats_page()?
        .goto_exercise(world.exercise_id()?)
        .await?;
    eventually_eq("the exercise heading", exercise.as_str(), || async {
        world.stats_page()?.exercise_heading().await
    })
    .await?;
    // Once any set has been logged the chart SVG renders; the "No progress data
    // yet" fallback only appears for an exercise with none.
    eventually("the progress chart is drawn", || async {
        world.stats_page()?.has_chart().await
    })
    .await
}

#[then("the PR list shows my exercise")]
async fn pr_list_shows_exercise(world: &mut LiftLogWorld) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world.stats_page()?.goto_prs().await?;
    eventually_eq("PR rows for my exercise", 1usize, || async {
        world.stats_page()?.pr_rows_for(&exercise).await
    })
    .await
}

#[then(expr = "the PR list shows {int} for my exercise in both the all-time and 1-month columns")]
async fn pr_list_shows_weight(world: &mut LiftLogWorld, weight: i64) -> Result<()> {
    let exercise = world.exercise()?.to_string();
    world.stats_page()?.goto_prs().await?;
    eventually_eq("the all-time PR", weight.to_string().as_str(), || async {
        world.stats_page()?.pr_cell(&exercise, "PR (All)").await
    })
    .await?;
    // Just logged, so the rolling 1-month window carries the same number.
    eventually_eq("the 1-month PR", weight.to_string().as_str(), || async {
        world.stats_page()?.pr_cell(&exercise, "PR (1M)").await
    })
    .await
}

// --- users ---------------------------------------------------------------

#[given("another user exists")]
async fn another_user_exists(world: &mut LiftLogWorld) -> Result<()> {
    let username = world.unique("subject");
    ensure_user(&username, PASSWORD).await?;
    world.other_user = Some(username);
    Ok(())
}

#[when("I create a new user via the admin UI")]
async fn create_user_via_ui(world: &mut LiftLogWorld) -> Result<()> {
    let username = world.unique("newbie");
    world.users_page()?.create(&username, PASSWORD).await?;
    eventually_eq("the URL", "/users", || async { world.path().await }).await?;
    world.other_user = Some(username);
    Ok(())
}

#[when("I promote that user to admin")]
async fn promote_user(world: &mut LiftLogWorld) -> Result<()> {
    confirm_user_action(world, "Promote", "Promote to admin").await
}

#[when("I delete that user")]
async fn delete_user(world: &mut LiftLogWorld) -> Result<()> {
    confirm_user_action(world, "Delete", "Delete user").await
}

#[when("I open the delete confirmation for that user")]
async fn open_delete_confirmation(world: &mut LiftLogWorld) -> Result<()> {
    let username = world.other_user()?.to_string();
    world.users_page()?.open_action(&username, "Delete").await
}

#[when("I confirm with the wrong password")]
async fn confirm_with_wrong_password(world: &mut LiftLogWorld) -> Result<()> {
    world
        .confirm_action_page()?
        .confirm("definitely-not-it", "Delete user")
        .await
}

#[then("I see a confirmation error")]
async fn see_confirmation_error(world: &mut LiftLogWorld) -> Result<()> {
    eventually("the confirmation error is showing", || async {
        Ok(contains(
            world.confirm_action_page()?.error().await?,
            "Password is incorrect",
        ))
    })
    .await
}

#[then("I see that user listed on the users page")]
async fn user_is_listed(world: &mut LiftLogWorld) -> Result<()> {
    let username = world.other_user()?.to_string();
    world.users_page()?.goto().await?;
    eventually_eq("rows for that user", 1usize, || async {
        world.users_page()?.rows_for(&username).await
    })
    .await
}

#[then("I see that user listed as Admin")]
async fn user_is_admin(world: &mut LiftLogWorld) -> Result<()> {
    let username = world.other_user()?.to_string();
    world.users_page()?.goto().await?;
    eventually("that user's row says admin", || async {
        Ok(world
            .users_page()?
            .row_text(&username)
            .await?
            .to_lowercase()
            .contains("admin"))
    })
    .await
}

#[then("I do not see that user on the users page")]
async fn user_is_not_listed(world: &mut LiftLogWorld) -> Result<()> {
    let username = world.other_user()?.to_string();
    world.users_page()?.goto().await?;
    eventually_eq("rows for that user", 0usize, || async {
        world.users_page()?.rows_for(&username).await
    })
    .await
}

#[then(expr = "I do not see the {string} button on the users page")]
async fn no_such_button(world: &mut LiftLogWorld, label: String) -> Result<()> {
    world.users_page()?.goto().await?;
    eventually_eq(&format!("`{label}` controls"), 0usize, || async {
        world.users_page()?.links_labelled(&label).await
    })
    .await
}

#[then("the users page does not let me delete my own account")]
async fn cannot_delete_myself(world: &mut LiftLogWorld) -> Result<()> {
    world.users_page()?.goto().await?;
    eventually("my own row is marked as mine", || async {
        world.users_page()?.marks_as_you(ADMIN).await
    })
    .await?;
    // A link now, not a submit button — the row action opens a confirmation
    // page rather than posting directly.
    eventually_eq("delete links on my own row", 0usize, || async {
        world.users_page()?.delete_links_for(ADMIN).await
    })
    .await
}

// --- helpers -------------------------------------------------------------

/// Signs in through the UI and waits for the dashboard, creating the account
/// first so a scenario can name a user without a `Given` for it.
async fn sign_in(world: &mut LiftLogWorld, username: &str, password: &str) -> Result<()> {
    ensure_user(username, password).await?;
    world.login_page()?.login(username, password).await?;
    see_dashboard(world).await
}

/// Creates an exercise and remembers what the server called it.
async fn add_exercise(world: &mut LiftLogWorld, name: &str, category: &str) -> Result<()> {
    let id = world.exercises_page()?.create(name, category).await?;
    world.exercise_name = Some(name.to_string());
    world.exercise_id = Some(id);
    Ok(())
}

/// Creates a workout dated today and remembers its id.
async fn create_workout(world: &mut LiftLogWorld) -> Result<()> {
    let id = world.new_workout_page()?.create_today().await?;
    world.workout_id = Some(id);
    Ok(())
}

/// Publishes a share link and remembers the path it produced.
async fn publish_share(world: &mut LiftLogWorld) -> Result<()> {
    let workout = world.workout()?;
    workout.goto().await?;
    workout.share().await?;
    eventually("a share link is on the page", || async {
        Ok(world.workout()?.share_url().await?.is_some())
    })
    .await?;
    world.share_url = world.workout()?.share_url().await?;
    Ok(())
}

/// Opens a users-page row action, confirms it with the admin's password, and
/// waits for the list to come back.
async fn confirm_user_action(world: &mut LiftLogWorld, link: &str, button: &str) -> Result<()> {
    let username = world.other_user()?.to_string();
    world.users_page()?.open_action(&username, link).await?;
    world
        .confirm_action_page()?
        .confirm(PASSWORD, button)
        .await?;
    eventually_eq("the URL", "/users", || async { world.path().await }).await
}

/// Waits for the browser to be back on the scenario's workout.
async fn on_the_workout(world: &mut LiftLogWorld) -> Result<()> {
    let expected = format!("/workouts/{}", world.workout_id()?);
    eventually_eq("the URL", expected.as_str(), || async {
        world.path().await
    })
    .await
}

/// Playwright's `toContainText`, with a missing element counting as no match.
fn contains(haystack: Option<String>, needle: &str) -> bool {
    haystack.is_some_and(|text| text.contains(needle))
}

/// Does the text match `YYYY-MM-DD HH:MM <suffix>`?
fn is_local_timestamp(text: &str, suffix: &str) -> bool {
    let Some(rest) = text.strip_suffix(suffix) else {
        return false;
    };
    let rest = rest.trim_end();
    let mut parts = rest.split(' ');
    let (Some(date), Some(time), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    matches_shape(date, "0000-00-00") && matches_shape(time, "00:00")
}

/// Is every digit position a digit, and every separator itself?
fn matches_shape(value: &str, shape: &str) -> bool {
    value.len() == shape.len()
        && value.chars().zip(shape.chars()).all(|(actual, expected)| {
            if expected == '0' {
                actual.is_ascii_digit()
            } else {
                actual == expected
            }
        })
}

// The no-JS path — the confirmation pages themselves — is covered in Rust
// integration tests (`workout_test.rs`, `exercises_test.rs`, `settings_test.rs`):
// each page renders the right consequence, is inert on GET, and refuses another
// user's row. A scripts-off browser scenario was tried in the Playwright suite
// and hung in CI, and a real browser adds little over those tests beyond proving
// that an `<a href>` navigates.
