//! `/users`: the admin-only list, its row actions, and their confirmation page.

use anyhow::{Context, Result};
use thirtyfour::prelude::*;

use super::{click_button, click_link_in, count, fill, goto, optional, quote, text};

/// The users list.
pub struct UsersPage<'a>(pub &'a WebDriver);

impl UsersPage<'_> {
    /// Navigates to the list.
    pub async fn goto(&self) -> Result<()> {
        goto(self.0, "/users").await
    }

    /// Creates a user through the admin form.
    pub async fn create(&self, username: &str, password: &str) -> Result<()> {
        goto(self.0, "/users/new").await?;
        fill(self.0, "username", username).await?;
        fill(self.0, "password", password).await?;
        click_button(self.0, "Create User").await
    }

    /// How many rows on the list name that user.
    pub async fn rows_for(&self, username: &str) -> Result<usize> {
        count(self.0, By::XPath(row_xpath(username))).await
    }

    /// The whole row for that user, as text.
    pub async fn row_text(&self, username: &str) -> Result<String> {
        text(self.0, By::XPath(row_xpath(username)))
            .await?
            .with_context(|| format!("no row for `{username}` on the users page"))
    }

    /// Opens a row action's confirmation page.
    ///
    /// Promote and delete are deliberately *not* enhanced with a
    /// `window.confirm()`: each opens a page that re-checks the admin's own
    /// password before acting, and re-authentication needs a real form.
    pub async fn open_action(&self, username: &str, action: &str) -> Result<()> {
        self.goto().await?;
        let row = optional(self.0, By::XPath(row_xpath(username)))
            .await?
            .with_context(|| format!("no row for `{username}` on the users page"))?;
        click_link_in(&row, action).await
    }

    /// How many delete links that user's row offers — 0 for the admin's own row.
    pub async fn delete_links_for(&self, username: &str) -> Result<usize> {
        count(
            self.0,
            By::XPath(format!(
                "{}//a[normalize-space(.)='Delete']",
                row_xpath(username)
            )),
        )
        .await
    }

    /// Is that row marked as the signed-in admin's own?
    pub async fn marks_as_you(&self, username: &str) -> Result<bool> {
        Ok(optional(
            self.0,
            By::XPath(format!(
                "{}//span[normalize-space(.)='(you)']",
                row_xpath(username)
            )),
        )
        .await?
        .is_some())
    }

    /// How many links with that text the page offers at all.
    ///
    /// Used by the negative path: a non-admin should not be offered
    /// "+ Add New User" in the first place, quite apart from the endpoint
    /// refusing them.
    pub async fn links_labelled(&self, label: &str) -> Result<usize> {
        count(
            self.0,
            By::XPath(format!("//a[normalize-space(.)={}]", quote(label))),
        )
        .await
    }
}

/// The re-authentication page a promote or delete opens.
pub struct ConfirmActionPage<'a>(pub &'a WebDriver);

impl ConfirmActionPage<'_> {
    /// Confirms with a password.
    ///
    /// `button` is the page's submit label, spelled out in full ("Delete user",
    /// not "Delete") — the point of the interstitial is that the admin reads
    /// what is about to happen.
    pub async fn confirm(&self, password: &str, button: &str) -> Result<()> {
        fill(self.0, "current_password", password).await?;
        click_button(self.0, button).await
    }

    /// The error banner's text, or `None` when the page is not showing one.
    pub async fn error(&self) -> Result<Option<String>> {
        text(self.0, By::Css(".error")).await
    }
}

/// XPath for the table row naming a user.
fn row_xpath(username: &str) -> String {
    format!(
        "//tr[td[@data-label='Username'][normalize-space(.)={}]]",
        quote(username)
    )
}
