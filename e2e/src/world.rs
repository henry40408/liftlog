//! The Cucumber world: one browser session per scenario, plus the handful of
//! ids a scenario builds up as it goes.
//!
//! The session is opened by a `before` hook rather than in `new`, so a failure
//! to start a browser is reported as a hook error against the scenario instead
//! of a panic inside the world constructor.
//!
//! `suffix` is the direct port of the old `scenarioState.unique(...)`. Every
//! scenario shares one server and one database — Playwright gave each worker
//! its own — so a fixture named `Squat` would collide with the same fixture in
//! a scenario running alongside it. Names carry a per-scenario suffix and each
//! scenario asserts only on what it built; "the lifter has no other workouts"
//! was never a safe assumption here and still is not.

use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use cucumber::World;
use thirtyfour::prelude::WebDriver;

use crate::browser::Browser;
use crate::pages::auth::{LoginPage, NavBar, SetupPage};
use crate::pages::dashboard::DashboardPage;
use crate::pages::exercises::ExercisesPage;
use crate::pages::settings::SettingsPage;
use crate::pages::stats::StatsPage;
use crate::pages::users::{ConfirmActionPage, UsersPage};
use crate::pages::workouts::{
    EditLogPage, EditWorkoutPage, NewWorkoutPage, WorkoutPage, WorkoutsPage,
};

/// Hands out the per-scenario suffix.
static SCENARIO: AtomicU64 = AtomicU64::new(0);

/// State shared by the steps of one scenario.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct LiftLogWorld {
    browser: Option<Browser>,
    /// Distinguishes this scenario's fixtures from every other scenario's.
    suffix: String,
    /// The workout the scenario created, if it created one.
    pub workout_id: Option<String>,
    /// The exercise the scenario created, if it created one.
    pub exercise_id: Option<String>,
    /// That exercise's name — the handle most assertions use.
    pub exercise_name: Option<String>,
    /// The share path published for the workout, if it was shared.
    pub share_url: Option<String>,
    /// The other account a users scenario acts on.
    pub other_user: Option<String>,
}

impl LiftLogWorld {
    fn new() -> Self {
        Self {
            browser: None,
            suffix: format!("{:x}", SCENARIO.fetch_add(1, Ordering::Relaxed)),
            workout_id: None,
            exercise_id: None,
            exercise_name: None,
            share_url: None,
            other_user: None,
        }
    }

    /// Opens the session for a scenario.
    pub async fn open(&mut self) -> Result<()> {
        self.browser = Some(Browser::open().await?);
        Ok(())
    }

    /// Ends the session, if one was opened.
    pub async fn close(&mut self) -> Result<()> {
        if let Some(browser) = self.browser.take() {
            browser.quit().await?;
        }
        Ok(())
    }

    /// A name no other scenario will produce.
    pub fn unique(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.suffix)
    }

    /// The scenario's browser.
    pub fn browser(&self) -> Result<&Browser> {
        self.browser
            .as_ref()
            .context("no browser session: the `before` hook did not open one")
    }

    /// The scenario's driver.
    pub fn driver(&self) -> Result<&WebDriver> {
        Ok(self.browser()?.driver())
    }

    /// The current URL's path, the shape the steps assert against.
    pub async fn path(&self) -> Result<String> {
        crate::pages::path(self.driver()?).await
    }

    /// The workout this scenario created.
    pub fn workout(&self) -> Result<WorkoutPage<'_>> {
        let id = self
            .workout_id
            .as_deref()
            .context("this step needs a workout, and the scenario has not created one")?;
        Ok(WorkoutPage::new(self.driver()?, id))
    }

    /// The id of the workout this scenario created.
    pub fn workout_id(&self) -> Result<&str> {
        self.workout_id
            .as_deref()
            .context("this step needs a workout, and the scenario has not created one")
    }

    /// The name of the exercise this scenario created.
    pub fn exercise(&self) -> Result<&str> {
        self.exercise_name
            .as_deref()
            .context("this step needs an exercise, and the scenario has not created one")
    }

    /// The id of the exercise this scenario created.
    pub fn exercise_id(&self) -> Result<&str> {
        self.exercise_id
            .as_deref()
            .context("this step needs an exercise, and the scenario has not created one")
    }

    /// The share path published for this scenario's workout.
    pub fn share_url(&self) -> Result<&str> {
        self.share_url
            .as_deref()
            .context("this step needs a shared workout, and the scenario has not shared one")
    }

    /// The other account this scenario is acting on.
    pub fn other_user(&self) -> Result<&str> {
        self.other_user
            .as_deref()
            .context("this step needs another user, and the scenario has not named one")
    }

    /// The workout metadata form for this scenario's workout.
    pub fn edit_workout(&self) -> Result<EditWorkoutPage<'_>> {
        Ok(EditWorkoutPage::new(self.driver()?, self.workout_id()?))
    }

    /// The login page.
    pub fn login_page(&self) -> Result<LoginPage<'_>> {
        Ok(LoginPage(self.driver()?))
    }

    /// The first-run setup page.
    pub fn setup_page(&self) -> Result<SetupPage<'_>> {
        Ok(SetupPage(self.driver()?))
    }

    /// The nav bar.
    pub fn nav(&self) -> Result<NavBar<'_>> {
        Ok(NavBar(self.driver()?))
    }

    /// The dashboard.
    pub fn dashboard(&self) -> Result<DashboardPage<'_>> {
        Ok(DashboardPage(self.driver()?))
    }

    /// The workouts list.
    pub fn workouts_page(&self) -> Result<WorkoutsPage<'_>> {
        Ok(WorkoutsPage(self.driver()?))
    }

    /// The new-workout form.
    pub fn new_workout_page(&self) -> Result<NewWorkoutPage<'_>> {
        Ok(NewWorkoutPage(self.driver()?))
    }

    /// The set form.
    pub fn edit_log_page(&self) -> Result<EditLogPage<'_>> {
        Ok(EditLogPage(self.driver()?))
    }

    /// The exercises list.
    pub fn exercises_page(&self) -> Result<ExercisesPage<'_>> {
        Ok(ExercisesPage(self.driver()?))
    }

    /// The settings page.
    pub fn settings_page(&self) -> Result<SettingsPage<'_>> {
        Ok(SettingsPage(self.driver()?))
    }

    /// The users list.
    pub fn users_page(&self) -> Result<UsersPage<'_>> {
        Ok(UsersPage(self.driver()?))
    }

    /// The re-authentication page a users row action opens.
    pub fn confirm_action_page(&self) -> Result<ConfirmActionPage<'_>> {
        Ok(ConfirmActionPage(self.driver()?))
    }

    /// The statistics pages.
    pub fn stats_page(&self) -> Result<StatsPage<'_>> {
        Ok(StatsPage(self.driver()?))
    }

    /// Requests a path as the signed-in user and reports the status.
    ///
    /// The browser's own session cookie rides along, so a 404 here is the one
    /// *this* user gets rather than the redirect a stranger would.
    pub async fn status_of(&self, path: &str) -> Result<u16> {
        let session = self.browser()?.session_cookie().await?;
        crate::http::status(path, session.as_deref()).await
    }
}
