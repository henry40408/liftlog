//! `/workouts`, `/workouts/new`, and the workout detail page with its set list.

use anyhow::{Context, Result, bail, ensure};
use thirtyfour::prelude::*;

use super::{
    all, click_button, click_link, click_link_in, count, displayed, fill, goto, optional, path,
    quote, select_by_label, set_value, text, value_of,
};
use crate::wait::{eventually, eventually_eq};

/// The workouts list (`/workouts`).
pub struct WorkoutsPage<'a>(pub &'a WebDriver);

impl WorkoutsPage<'_> {
    pub async fn goto(&self) -> Result<()> {
        goto(self.0, "/workouts").await
    }

    /// How many links on the page point at that workout — 0 or 1 in practice,
    /// which is what both the "is listed" and "is not listed" steps ask.
    pub async fn links_to(&self, id: &str) -> Result<usize> {
        count(self.0, By::Css(format!("a[href=\"/workouts/{id}\"]"))).await
    }

    /// The empty-state text, or `None` when the list has workouts in it.
    pub async fn empty_state(&self) -> Result<Option<String>> {
        text(self.0, By::Css(".empty-state")).await
    }
}

/// The new-workout form (`/workouts/new`).
pub struct NewWorkoutPage<'a>(pub &'a WebDriver);

impl NewWorkoutPage<'_> {
    /// Creates a workout dated today and returns the id it was given.
    ///
    /// The date field is pre-filled by the server, so this submits the form as
    /// it stands — the same one click the old step made.
    pub async fn create_today(&self) -> Result<String> {
        goto(self.0, "/workouts/new").await?;
        click_button(self.0, "Create Workout").await?;
        // The POST redirects to the new workout; the click returns before the
        // browser has followed it.
        eventually("the browser reaches the new workout", || async {
            Ok(workout_id(self.0).await.is_ok())
        })
        .await?;
        workout_id(self.0).await
    }
}

/// A workout's detail page (`/workouts/{id}`), and everything hanging off it.
pub struct WorkoutPage<'a> {
    driver: &'a WebDriver,
    id: String,
}

impl<'a> WorkoutPage<'a> {
    pub fn new(driver: &'a WebDriver, id: impl Into<String>) -> Self {
        Self {
            driver,
            id: id.into(),
        }
    }

    pub async fn goto(&self) -> Result<()> {
        goto(self.driver, &format!("/workouts/{}", self.id)).await
    }

    /// Picks an exercise in the Add Set form.
    ///
    /// Also what drives the "last weight" hint: the page's script listens for
    /// this `change` and fetches the previous set's figures.
    pub async fn select_exercise(&self, name: &str) -> Result<()> {
        select_by_label(self.driver, "exercise_id", name).await
    }

    /// The values an Add Set field's `<datalist>` offers, in document order.
    ///
    /// Read through the `list` attribute rather than a hardcoded list id, so
    /// this fails if the field stops pointing at a list at all — which is the
    /// half a browser can check and a markup assertion cannot. Filling the
    /// field is unaffected either way: a datalist is a hint, and `fill` drives
    /// these inputs exactly as it did before they had one.
    pub async fn suggestions(&self, field: &str) -> Result<Vec<String>> {
        let list = self
            .driver
            .find(By::Id(field))
            .await?
            .attr("list")
            .await?
            .with_context(|| format!("the `{field}` field points at no datalist"))?;
        let mut values = Vec::new();
        for option in all(self.driver, By::Css(format!("#{list} option"))).await? {
            if let Some(value) = option.attr("value").await? {
                values.push(value);
            }
        }
        ensure!(
            !values.is_empty(),
            "the `{field}` field points at `{list}`, which offers nothing"
        );
        Ok(values)
    }

    /// Fills the Add Set form and submits it, then waits for the new row.
    ///
    /// Counted, not merely "a row exists": a second set of the same exercise
    /// would find the first row and return before its own POST had landed,
    /// leaving the next navigation to cancel it. The form posts and re-renders
    /// the page, so the row count is the only thing that says the write is done.
    pub async fn log_set(
        &self,
        exercise: &str,
        weight: &str,
        reps: &str,
        rpe: Option<&str>,
    ) -> Result<()> {
        let before = self.rows(exercise).await?.len();
        self.select_exercise(exercise).await?;
        fill(self.driver, "weight", weight).await?;
        fill(self.driver, "reps", reps).await?;
        if let Some(rpe) = rpe {
            fill(self.driver, "rpe", rpe).await?;
        }
        click_button(self.driver, "Add Set").await?;
        eventually_eq("the set rows after adding one", before + 1, || async {
            Ok(self.rows(exercise).await?.len())
        })
        .await
    }

    /// Every set row logged against that exercise.
    pub async fn rows(&self, exercise: &str) -> Result<Vec<WebElement>> {
        all(self.driver, By::XPath(row_xpath(exercise))).await
    }

    /// The first set row for that exercise.
    pub async fn row(&self, exercise: &str) -> Result<WebElement> {
        self.rows(exercise)
            .await?
            .into_iter()
            .next()
            .with_context(|| format!("no set row for `{exercise}`"))
    }

    /// One cell of the first row, by the `set-cell-*` suffix that names it.
    pub async fn cell(&self, exercise: &str, cell: &str) -> Result<String> {
        Ok(self
            .row(exercise)
            .await?
            .find(By::Css(format!(".set-cell-{cell}")))
            .await?
            .text()
            .await?)
    }

    /// The `set-cell-set` value of every row for that exercise, sorted.
    ///
    /// Sorted because the assertion is about which numbers were handed out, not
    /// about the order the list happens to render them in.
    pub async fn set_numbers(&self, exercise: &str) -> Result<Vec<String>> {
        let mut numbers = Vec::new();
        for row in self.rows(exercise).await? {
            numbers.push(row.find(By::Css(".set-cell-set")).await?.text().await?);
        }
        numbers.sort();
        Ok(numbers)
    }

    /// The PR badge on the first row, or `None` when the set is not a record.
    pub async fn pr_badge(&self, exercise: &str) -> Result<Option<String>> {
        let row = self.row(exercise).await?;
        match row.query(By::Css(".pr-badge")).nowait().first_opt().await? {
            Some(badge) => Ok(Some(badge.text().await?)),
            None => Ok(None),
        }
    }

    /// Opens the edit form for the first set of that exercise.
    pub async fn open_edit_set(&self, exercise: &str) -> Result<()> {
        let row = self.row(exercise).await?;
        click_link_in(&row, "Edit").await?;
        displayed(
            self.driver,
            By::XPath("//h1[normalize-space(.)='Edit Set']"),
        )
        .await?;
        Ok(())
    }

    /// Deletes the first set of that exercise.
    ///
    /// The trigger is a link to a confirmation page; with scripts on,
    /// `base.html` intercepts it and asks in a `window.confirm()` that the
    /// session accepts automatically. Addressed by `aria-label` because its
    /// visible text is a bare `×`.
    ///
    /// Waits for the row to leave the list rather than for a URL: the POST
    /// re-renders the page it was already on, so there is no navigation to
    /// watch — and a caller that navigated away immediately would cancel the
    /// request instead of completing it.
    pub async fn delete_set(&self, exercise: &str) -> Result<()> {
        self.row(exercise)
            .await?
            .find(By::Css("a[aria-label=\"Delete set\"]"))
            .await?
            .click()
            .await?;
        eventually("the set row leaves the workout", || async {
            Ok(self.rows(exercise).await?.is_empty())
        })
        .await
    }

    /// Clicks Clone on the first set of that exercise.
    ///
    /// A link to `?prefill=<log id>` that the page's script intercepts to fill
    /// the form in place — both paths land on the same pre-filled form, which is
    /// what the assertion checks.
    pub async fn clone_set(&self, exercise: &str) -> Result<()> {
        let row = self.row(exercise).await?;
        click_link_in(&row, "Clone").await
    }

    /// Deletes the whole workout. Same intercepted-link shape as the set delete.
    pub async fn delete(&self) -> Result<()> {
        click_link(self.driver, "Delete").await
    }

    /// The "last weight" hint's RPE chip, which only a script draws.
    pub async fn last_info_rpe(&self) -> Result<Option<String>> {
        text(self.driver, By::Css("#exercise-last-weight-info .rpe-chip")).await
    }

    /// Publishes a share link for the workout.
    pub async fn share(&self) -> Result<()> {
        displayed(
            self.driver,
            By::XPath("//form[contains(@action,'/share')]//button[normalize-space(.)='Share']"),
        )
        .await?
        .click()
        .await?;
        Ok(())
    }

    /// The public share path, or `None` when the workout is not shared.
    ///
    /// Picked by `href` rather than by position: Revoke Share is a link inside
    /// the same block now, so "the first anchor" is no longer the share URL.
    pub async fn share_url(&self) -> Result<Option<String>> {
        match optional(self.driver, By::Css(".share-info a[href^=\"/shared/\"]")).await? {
            Some(link) => Ok(link.attr("href").await?),
            None => Ok(None),
        }
    }

    /// Withdraws the share link.
    pub async fn revoke_share(&self) -> Result<()> {
        click_link(self.driver, "Revoke Share").await
    }

    /// Is a share block on the page at all?
    pub async fn has_share_info(&self) -> Result<bool> {
        Ok(count(self.driver, By::Css(".share-info")).await? > 0)
    }

    /// The `<h1>`, which the detail page renders as the workout's date.
    pub async fn heading(&self) -> Result<String> {
        Ok(displayed(self.driver, By::Css("h1")).await?.text().await?)
    }

    /// The notes line under the heading.
    pub async fn notes(&self) -> Result<Option<String>> {
        text(self.driver, By::Css(".subtitle em")).await
    }

    /// The exercise the Add Set form currently has selected, as its id.
    pub async fn prefilled_exercise(&self) -> Result<String> {
        value_of(self.driver, "exercise_id").await
    }

    /// The weight the Add Set form currently holds.
    pub async fn prefilled_weight(&self) -> Result<String> {
        value_of(self.driver, "weight").await
    }

    /// The reps the Add Set form currently holds.
    pub async fn prefilled_reps(&self) -> Result<String> {
        value_of(self.driver, "reps").await
    }
}

/// The workout metadata form (`/workouts/{id}/edit`).
pub struct EditWorkoutPage<'a> {
    driver: &'a WebDriver,
    id: String,
}

impl<'a> EditWorkoutPage<'a> {
    pub fn new(driver: &'a WebDriver, id: impl Into<String>) -> Self {
        Self {
            driver,
            id: id.into(),
        }
    }

    /// Rewrites the date and notes, and submits.
    ///
    /// The date goes in by assignment rather than by typing — see
    /// [`super::set_value`].
    pub async fn save(&self, date: &str, notes: &str) -> Result<()> {
        goto(self.driver, &format!("/workouts/{}/edit", self.id)).await?;
        set_value(self.driver, "date", date).await?;
        fill(self.driver, "notes", notes).await?;
        click_button(self.driver, "Save Changes").await
    }
}

/// The set form (`/workouts/{id}/logs/{log}/edit`), reached from a row's Edit.
pub struct EditLogPage<'a>(pub &'a WebDriver);

impl EditLogPage<'_> {
    /// Rewrites the weight and reps, and submits.
    pub async fn save(&self, weight: &str, reps: &str) -> Result<()> {
        fill(self.0, "weight", weight).await?;
        fill(self.0, "reps", reps).await?;
        click_button(self.0, "Save Changes").await
    }
}

/// The workout id in the browser's current URL.
///
/// # Errors
///
/// Fails when the browser is not on a workout detail page — which is the useful
/// failure for "Create Workout did not land where it should have".
pub async fn workout_id(driver: &WebDriver) -> Result<String> {
    let path = path(driver).await?;
    // The id has to look like one, not merely sit in that position: `/workouts/new`
    // is also a single segment under `/workouts/`, and accepting it made "wait
    // until the create redirect lands" pass before the click had gone anywhere.
    let id = path
        .strip_prefix("/workouts/")
        .filter(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    match id {
        Some(id) => Ok(id.to_string()),
        None => bail!("not on a workout detail page: {path}"),
    }
}

/// XPath for the set rows naming a given exercise.
///
/// Scoped to the exercise cell rather than the whole row: a row also carries
/// the weight and reps, and a bare `contains(., name)` would match a different
/// exercise whose name is a prefix of this one.
fn row_xpath(exercise: &str) -> String {
    format!(
        "//div[contains(@class,'set-row')]\
         [.//div[contains(@class,'set-cell-exercise')][contains(normalize-space(.), {})]]",
        quote(exercise)
    )
}
