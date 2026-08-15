//! `/auth/login`, `/auth/setup`, and the nav bar's Sign Out.

use anyhow::Result;
use thirtyfour::prelude::*;

use super::{click_button, disable_validation, displayed, fill, goto, optional, path, text};
use crate::wait::eventually_eq;

/// The login form (`/auth/login`).
pub struct LoginPage<'a>(pub &'a WebDriver);

impl LoginPage<'_> {
    /// Navigates to the login page.
    pub async fn goto(&self) -> Result<()> {
        goto(self.0, "/auth/login").await
    }

    /// Fills the form and submits it, from wherever the browser is.
    pub async fn login(&self, username: &str, password: &str) -> Result<()> {
        self.goto().await?;
        fill(self.0, "username", username).await?;
        fill(self.0, "password", password).await?;
        click_button(self.0, "Login").await
    }

    /// Is the login form on screen? Both halves of "I see the login page".
    pub async fn is_showing(&self) -> Result<bool> {
        Ok(
            optional(self.0, By::XPath("//button[normalize-space(.)='Login']"))
                .await?
                .is_some(),
        )
    }

    /// The error banner's text, or `None` when the page is not showing one.
    pub async fn error(&self) -> Result<Option<String>> {
        text(self.0, By::Css(".error")).await
    }
}

/// The first-run account form (`/auth/setup`).
pub struct SetupPage<'a>(pub &'a WebDriver);

impl SetupPage<'_> {
    /// Navigates to the setup page.
    pub async fn goto(&self) -> Result<()> {
        goto(self.0, "/auth/setup").await
    }

    /// Fills the form and submits it, with the browser's own validation off.
    ///
    /// The password field carries `minlength`, so the scenarios that submit a
    /// deliberately-short password would otherwise never reach the server —
    /// and the server-side policy check is the control under test.
    pub async fn submit(&self, username: &str, password: &str) -> Result<()> {
        self.goto().await?;
        disable_validation(self.0, "form").await?;
        fill(self.0, "username", username).await?;
        fill(self.0, "password", password).await?;
        click_button(self.0, "Create Account").await
    }

    /// Is the setup form on screen?
    pub async fn is_showing(&self) -> Result<bool> {
        Ok(optional(
            self.0,
            By::XPath("//button[normalize-space(.)='Create Account']"),
        )
        .await?
        .is_some())
    }

    /// The error banner's text, or `None` when the page is not showing one.
    pub async fn error(&self) -> Result<Option<String>> {
        text(self.0, By::Css(".error")).await
    }
}

/// The nav bar, present on every signed-in page.
pub struct NavBar<'a>(pub &'a WebDriver);

impl NavBar<'_> {
    /// Submits the Sign Out form and waits to land on the login page.
    ///
    /// A real `<button>` in a POST form, not a link: signing out is a
    /// state-changing action and works with scripts off.
    ///
    /// The wait is what makes a following sign-in honest. Without it the next
    /// navigation cancels the logout, the old session survives, and
    /// `/auth/login` bounces straight back to the dashboard — which surfaces as
    /// a login form that has no username field rather than as a failed logout.
    pub async fn sign_out(&self) -> Result<()> {
        displayed(self.0, By::Css("button.sign-out-btn"))
            .await?
            .click()
            .await?;
        eventually_eq("the URL after signing out", "/auth/login", || async {
            path(self.0).await
        })
        .await
    }
}
