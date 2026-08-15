//! `/stats`, `/stats/exercise/{id}` and `/stats/prs`.

use anyhow::{Context, Result};
use thirtyfour::prelude::*;

use super::{count, dom_text, goto, optional, quote, text};

/// The statistics pages.
pub struct StatsPage<'a>(pub &'a WebDriver);

impl StatsPage<'_> {
    /// Navigates to the overview.
    pub async fn goto_overview(&self) -> Result<()> {
        goto(self.0, "/stats").await
    }

    /// Navigates to one exercise's progress page.
    pub async fn goto_exercise(&self, id: &str) -> Result<()> {
        goto(self.0, &format!("/stats/exercise/{id}")).await
    }

    /// Navigates to the personal-records list.
    pub async fn goto_prs(&self) -> Result<()> {
        goto(self.0, "/stats/prs").await
    }

    /// Is the overview's own heading and summary grid on screen?
    pub async fn overview_is_showing(&self) -> Result<bool> {
        let heading = optional(self.0, By::XPath("//h1[normalize-space(.)='Statistics']")).await?;
        let grid = optional(self.0, By::Css(".stats-grid")).await?;
        Ok(heading.is_some() && grid.is_some())
    }

    /// The `<h1>` of the exercise page, which is the exercise's name.
    ///
    /// Read as DOM text: the heading is styled `text-transform: uppercase`, so
    /// the rendered text would never equal the name the scenario created.
    pub async fn exercise_heading(&self) -> Result<String> {
        dom_text(self.0, "h1")
            .await?
            .context("the exercise stats page has no heading")
    }

    /// Is the progress chart drawn?
    ///
    /// The SVG renders once any set has been logged; the "No progress data yet"
    /// fallback only appears for an exercise with none.
    pub async fn has_chart(&self) -> Result<bool> {
        Ok(optional(self.0, By::Id("exercise-chart")).await?.is_some())
    }

    /// How many rows of the PR list name that exercise.
    pub async fn pr_rows_for(&self, exercise: &str) -> Result<usize> {
        count(self.0, By::XPath(pr_row_xpath(exercise))).await
    }

    /// One column of that exercise's PR row, addressed by its `data-label`.
    pub async fn pr_cell(&self, exercise: &str, label: &str) -> Result<String> {
        let xpath = format!(
            "{}/td[@data-label={}]",
            pr_row_xpath(exercise),
            quote(label)
        );
        text(self.0, By::XPath(xpath))
            .await?
            .with_context(|| format!("no `{label}` cell for `{exercise}` on the PR list"))
    }
}

/// XPath for the PR row naming an exercise.
fn pr_row_xpath(exercise: &str) -> String {
    format!(
        "//tbody/tr[td[@data-label='Exercise']//a[normalize-space(.)={}]]",
        quote(exercise)
    )
}
