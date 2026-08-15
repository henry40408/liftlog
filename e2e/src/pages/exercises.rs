//! `/exercises` and its create/edit forms.

use anyhow::{Context, Result, bail};
use thirtyfour::prelude::*;

use super::{
    all, click_button, click_link_in, count, fill, goto, optional, path, quote, select_by_value,
};
use crate::wait::{eventually, eventually_eq};

/// The exercises list and the forms reached from it.
pub struct ExercisesPage<'a>(pub &'a WebDriver);

impl ExercisesPage<'_> {
    /// Navigates to the list.
    pub async fn goto(&self) -> Result<()> {
        goto(self.0, "/exercises").await
    }

    /// Creates an exercise and returns the id the server gave it.
    ///
    /// The id is read back off the list, where each exercise links to its own
    /// stats page — the only place the server hands it to the client.
    pub async fn create(&self, name: &str, category: &str) -> Result<String> {
        goto(self.0, "/exercises/new").await?;
        fill(self.0, "name", name).await?;
        select_by_value(self.0, "category", category).await?;
        click_button(self.0, "Add Exercise").await?;
        self.settle().await?;
        self.id_of(name).await
    }

    /// Renames the exercise, returning nothing — the caller holds the new name.
    pub async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.goto().await?;
        let row = self.row(from).await?;
        click_link_in(&row, "Edit").await?;
        fill(self.0, "name", to).await?;
        click_button(self.0, "Save Changes").await
    }

    /// Deletes the exercise.
    ///
    /// The trigger is a link to a confirmation page that `base.html` intercepts
    /// with a `window.confirm()`, which this session accepts automatically.
    ///
    /// Waits for the entry to leave the list rather than for a URL: the POST
    /// re-renders the page it was already on, so there is no navigation to
    /// watch — and a caller that navigated away immediately would cancel the
    /// request instead of completing it.
    pub async fn delete(&self, name: &str) -> Result<()> {
        self.goto().await?;
        let row = self.row(name).await?;
        click_link_in(&row, "Delete").await?;
        eventually(&format!("`{name}` leaves the list"), || async {
            Ok(self.entries_named(name).await? == 0)
        })
        .await
    }

    /// Waits for a form post to land back on the list.
    ///
    /// WebDriver's click does not reliably block until a form's redirect has
    /// been followed, and the next navigation would cancel the request that is
    /// still in flight — which showed up as an exercise that was never created.
    async fn settle(&self) -> Result<()> {
        eventually_eq("the URL after the form post", "/exercises", || async {
            path(self.0).await
        })
        .await
    }

    /// How many entries on the list carry that name.
    pub async fn entries_named(&self, name: &str) -> Result<usize> {
        count(self.0, By::XPath(link_xpath(name))).await
    }

    /// Every name currently on the list, for the failure messages.
    pub async fn listed_names(&self) -> Result<Vec<String>> {
        let links = all(self.0, By::Css(".exercise-item > a")).await?;
        let mut names = Vec::with_capacity(links.len());
        for link in links {
            names.push(link.text().await?);
        }
        Ok(names)
    }

    /// The exercise's id, read off its link to `/stats/exercise/{id}`.
    pub async fn id_of(&self, name: &str) -> Result<String> {
        self.goto().await?;
        let Some(entry) = optional(self.0, By::XPath(link_xpath(name))).await? else {
            bail!(
                "`{name}` is not on /exercises, which lists {:?}",
                self.listed_names().await?
            );
        };
        let href = entry
            .attr("href")
            .await?
            .with_context(|| format!("the `{name}` entry has no href"))?;
        href.strip_prefix("/stats/exercise/")
            .map(ToString::to_string)
            .with_context(|| format!("`{name}` links to {href}, not to its stats page"))
    }

    /// The list row containing that exercise.
    async fn row(&self, name: &str) -> Result<WebElement> {
        let xpath = format!(
            "//div[contains(@class,'exercise-item')][.//a[normalize-space(.)={}]]",
            quote(name)
        );
        optional(self.0, By::XPath(xpath))
            .await?
            .with_context(|| format!("no exercise named `{name}` on the list"))
    }
}

/// XPath for an exercise's own link, which is the one carrying its name.
fn link_xpath(name: &str) -> String {
    format!(
        "//div[contains(@class,'exercise-item')]/a[normalize-space(.)={}]",
        quote(name)
    )
}
