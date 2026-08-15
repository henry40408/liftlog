//! `/settings`: the password form and the active-sessions table.

use anyhow::{Context, Result};
use thirtyfour::prelude::*;

use super::{click_button, click_link, disable_validation, fill, goto, optional, text};

/// The sessions table, picked by its own column header.
///
/// `/settings` renders two `.data-table`s — active sessions and application
/// info — so a positional selector would silently start asserting about the
/// wrong one the day a third is added.
const SESSIONS_TABLE: &str =
    "//table[contains(@class,'data-table')][.//th[normalize-space(.)='Device']]";

/// The settings page.
pub struct SettingsPage<'a>(pub &'a WebDriver);

impl SettingsPage<'_> {
    /// Navigates to the settings page.
    pub async fn goto(&self) -> Result<()> {
        goto(self.0, "/settings").await
    }

    /// Fills the change-password form and submits it.
    ///
    /// Client-side validation is turned off first: the new-password field
    /// carries `minlength`, and the scenarios submitting a deliberately-short
    /// password are there to lock the *server's* check.
    pub async fn change_password(&self, current: &str, next: &str, confirm: &str) -> Result<()> {
        self.goto().await?;
        disable_validation(self.0, "form[action=\"/settings/password\"]").await?;
        fill(self.0, "current_password", current).await?;
        fill(self.0, "new_password", next).await?;
        fill(self.0, "confirm_password", confirm).await?;
        click_button(self.0, "Change Password").await
    }

    /// The success banner's text, or `None` when the page is not showing one.
    pub async fn success(&self) -> Result<Option<String>> {
        text(self.0, By::Css(".alert-success")).await
    }

    /// The error banner's text, or `None` when the page is not showing one.
    pub async fn error(&self) -> Result<Option<String>> {
        text(self.0, By::Css(".error")).await
    }

    /// Ends every session but this one.
    ///
    /// The trigger is a link to a confirmation page, intercepted by `base.html`
    /// into a `window.confirm()`. Its POST re-renders `/settings` in place
    /// rather than redirecting, so the success banner is what says it worked.
    pub async fn log_out_other_devices(&self) -> Result<()> {
        self.goto().await?;
        click_link(self.0, "Log out all other devices").await
    }

    /// How many rows the active-sessions table has.
    pub async fn session_count(&self) -> Result<usize> {
        Ok(self
            .0
            .query(By::XPath(format!("{SESSIONS_TABLE}//tbody/tr")))
            .nowait()
            .all_from_selector()
            .await?
            .len())
    }

    /// Does the table mark one of the rows as this browser's own session?
    pub async fn marks_current_device(&self) -> Result<bool> {
        Ok(optional(
            self.0,
            By::XPath(format!(
                "{SESSIONS_TABLE}//strong[normalize-space(.)='This device']"
            )),
        )
        .await?
        .is_some())
    }

    /// The `data-label` of every cell in the first session row.
    ///
    /// Under 480px the table collapses to cards and the column headers are
    /// hidden; `td::before` re-prints them from `data-label`. Without it the two
    /// timestamps sit next to each other with nothing telling them apart.
    pub async fn first_row_labels(&self) -> Result<Vec<String>> {
        let cells = self
            .0
            .query(By::XPath(format!("{SESSIONS_TABLE}//tbody/tr[1]/td")))
            .nowait()
            .all_from_selector()
            .await?;

        let mut labels = Vec::with_capacity(cells.len());
        for cell in cells {
            labels.push(
                cell.attr("data-label")
                    .await?
                    .context("a session cell carries no data-label")?,
            );
        }
        Ok(labels)
    }

    /// The rendered text of both timestamps in the first session row.
    ///
    /// Server-rendered as UTC and rewritten by `base.html` into the browser's
    /// own zone, which is the behaviour the timezone scenario is checking.
    pub async fn first_row_times(&self) -> Result<Vec<String>> {
        let times = self
            .0
            .query(By::XPath(format!("{SESSIONS_TABLE}//tbody/tr[1]//time")))
            .nowait()
            .all_from_selector()
            .await?;

        let mut rendered = Vec::with_capacity(times.len());
        for time in times {
            rendered.push(time.text().await?);
        }
        Ok(rendered)
    }
}
