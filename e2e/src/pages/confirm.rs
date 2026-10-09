//! The server-rendered confirmation page a destructive link opens when no
//! script intercepts it (`templates/confirm.html`).

use anyhow::Result;
use thirtyfour::prelude::*;

use super::{click_link, displayed, path};
use crate::wait::eventually;

/// The "are you sure" page.
pub struct ConfirmPage<'a>(pub &'a WebDriver);

impl ConfirmPage<'_> {
    /// Waits for the page and returns the path it is served from.
    pub async fn path(&self) -> Result<String> {
        displayed(self.0, By::Css("main form[method=post] .btn-danger")).await?;
        path(self.0).await
    }

    /// Presses the button that carries out the action, then waits for the
    /// redirect away from the confirmation page so the next navigation does
    /// not cancel the POST.
    pub async fn confirm(&self) -> Result<()> {
        let here = self.path().await?;
        displayed(self.0, By::Css("main form[method=post] .btn-danger"))
            .await?
            .click()
            .await?;
        eventually(
            "the POST redirects away from the confirmation page",
            || async { Ok(path(self.0).await? != here) },
        )
        .await
    }

    /// Backs out through the Cancel link.
    pub async fn cancel(&self) -> Result<()> {
        click_link(self.0, "Cancel").await
    }
}
