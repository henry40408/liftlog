//! The dashboard (`/`).

use anyhow::{Context, Result};
use thirtyfour::prelude::*;

use super::{count, goto, optional, quote, text, top_of};

pub struct DashboardPage<'a>(pub &'a WebDriver);

impl DashboardPage<'_> {
    pub async fn goto(&self) -> Result<()> {
        goto(self.0, "/").await
    }

    /// Is the `<h1>Dashboard</h1>` on screen? The signed-in marker.
    pub async fn is_showing(&self) -> Result<bool> {
        Ok(
            optional(self.0, By::XPath("//h1[normalize-space(.)='Dashboard']"))
                .await?
                .is_some(),
        )
    }

    /// Is the workout linked from the Recent Workouts list?
    pub async fn lists_workout(&self, id: &str) -> Result<bool> {
        Ok(count(
            self.0,
            By::Css(format!(".workout-list a[href=\"/workouts/{id}\"]")),
        )
        .await?
            > 0)
    }

    /// Is the `<h2>Recent Workouts</h2>` heading there?
    pub async fn has_recent_heading(&self) -> Result<bool> {
        Ok(optional(
            self.0,
            By::XPath("//h2[normalize-space(.)='Recent Workouts']"),
        )
        .await?
        .is_some())
    }

    /// The number on the summary card with the given label.
    pub async fn stat(&self, label: &str) -> Result<String> {
        let xpath = format!(
            "//div[contains(@class,'stat-card')][.//div[contains(@class,'stat-label')]\
             [normalize-space(.)={}]]//div[contains(@class,'stat-value')]",
            quote(label)
        );
        text(self.0, By::XPath(xpath))
            .await?
            .with_context(|| format!("no summary card labelled `{label}`"))
    }

    /// Do the quick actions render above both the summary and the workout list?
    pub async fn actions_lead(&self) -> Result<bool> {
        let actions = top_of(self.0, ".actions-lead").await?;
        let stats = top_of(self.0, ".stats-grid").await?;
        let recent = top_of(self.0, "h2").await?;
        Ok(actions < stats && actions < recent)
    }
}
