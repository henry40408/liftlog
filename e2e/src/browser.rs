//! The browser session. `WebDriver::managed` downloads chromedriver but not the
//! browser, so a local Chrome or Chromium is a prerequisite.
//!
//! `unhandledPromptBehavior: accept` auto-accepts the `window.confirm()` that
//! `base.html` raises for `a[data-confirm]`; otherwise the next command fails
//! with "unexpected alert open".

use std::time::Duration;

use anyhow::{Context, Result};
use thirtyfour::prelude::*;

/// How long a query waits. Sized for a two-core CI runner driving several
/// browsers; only a genuine failure pays it in full.
pub const WAIT_TIMEOUT: Duration = Duration::from_secs(30);

pub const WAIT_INTERVAL: Duration = Duration::from_millis(100);

/// Above the 480px breakpoint where `.data-table` collapses into cards, which
/// the active-sessions scenarios depend on.
const WINDOW: (u32, u32) = (1280, 720);

/// A browser session, scoped to one scenario.
#[derive(Debug)]
pub struct Browser {
    driver: WebDriver,
}

impl Browser {
    /// Starts a headless session.
    ///
    /// # Errors
    ///
    /// Fails when no local browser is installed, when the driver cannot be
    /// downloaded, or when the session cannot be created.
    pub async fn open() -> Result<Self> {
        let mut caps = DesiredCapabilities::chrome();
        caps.set_headless()?;
        caps.add_arg(&format!("--window-size={},{}", WINDOW.0, WINDOW.1))?;
        // Containers get a 64 MB /dev/shm by default, which Chrome outgrows.
        caps.add_arg("--disable-dev-shm-usage")?;
        caps.as_mut().set("unhandledPromptBehavior", "accept")?;

        let driver = WebDriver::managed(caps).await.context(
            "could not start a browser session — a local Chrome or Chromium is required \
             (`brew install --cask ungoogled-chromium`, or `google-chrome` on CI); \
             the driver manager downloads only the driver",
        )?;

        Ok(Self { driver })
    }

    /// Opens and closes one session so the driver is downloaded once. On a
    /// cold cache, parallel sessions would each download it and stall on its
    /// lock file.
    ///
    /// # Errors
    ///
    /// Fails for the same reasons [`Browser::open`] does.
    pub async fn prepare() -> Result<()> {
        Self::open().await?.quit().await
    }

    /// The underlying session, for the page objects.
    pub fn driver(&self) -> &WebDriver {
        &self.driver
    }

    /// Pins the browser's timezone via CDP on the live session.
    ///
    /// # Errors
    ///
    /// Fails when the CDP command is refused — an unknown IANA zone, most
    /// likely.
    pub async fn set_timezone(&self, timezone: &str) -> Result<()> {
        self.driver
            .cdp()
            .send_raw(
                "Emulation.setTimezoneOverride",
                serde_json::json!({ "timezoneId": timezone }),
            )
            .await?;
        Ok(())
    }

    /// The session cookie the browser is holding, if it is signed in.
    ///
    /// Used by [`crate::http`] to assert status codes as this user.
    ///
    /// # Errors
    ///
    /// Fails only on a driver error.
    pub async fn session_cookie(&self) -> Result<Option<String>> {
        // Plain HTTP, so not the `__Host-` variant.
        match self.driver.get_named_cookie("session").await {
            Ok(cookie) => Ok(Some(cookie.value.clone())),
            Err(_) => Ok(None),
        }
    }

    /// Drops every cookie, which is how a scenario becomes a different user.
    ///
    /// # Errors
    ///
    /// Fails when the driver refuses.
    pub async fn clear_cookies(&self) -> Result<()> {
        self.driver.delete_all_cookies().await?;
        Ok(())
    }

    /// Ends the session.
    ///
    /// # Errors
    ///
    /// Fails when the driver refuses to close.
    pub async fn quit(self) -> Result<()> {
        self.driver.quit().await?;
        Ok(())
    }
}
