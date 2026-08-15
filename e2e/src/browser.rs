//! The browser session, and the two things the suite needs beyond plain
//! navigation.
//!
//! `WebDriver::managed` downloads and supervises a matching chromedriver
//! itself, so nothing has to be installed alongside the tests — but it does
//! *not* download the browser, unlike the Playwright setup this replaces. A
//! Chrome or Chromium in one of the well-known locations is a prerequisite now;
//! [`Browser::open`] says so in as many words when it is missing, because the
//! raw driver error does not.
//!
//! `unhandledPromptBehavior: accept` stands in for Playwright's
//! `page.once('dialog', d => d.accept())`. Every destructive trigger in LiftLog
//! is an `<a href>` to a confirmation page that `base.html` intercepts with a
//! `window.confirm()`; a WebDriver session with the default prompt behaviour
//! fails the *next* command with "unexpected alert open" instead. Accepting is
//! what the old steps did, one dialog at a time — the difference is that this
//! cannot be forgotten before a click.

use std::time::Duration;

use anyhow::{Context, Result};
use thirtyfour::prelude::*;

/// How long a query waits for a condition before giving up.
///
/// Only ever paid in full by a genuine failure, so it is set for the slowest
/// machine that runs this rather than the fastest: locally every wait settles
/// in well under a second, while a two-core CI runner driving several browsers
/// takes considerably longer to land a navigation.
pub const WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// How often a query re-checks while waiting.
pub const WAIT_INTERVAL: Duration = Duration::from_millis(100);

/// Viewport, matching the `Desktop Chrome` device the Playwright project used.
///
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
             unlike Playwright, the driver manager downloads only the driver",
        )?;

        Ok(Self { driver })
    }

    /// Downloads and starts the driver once, before any scenario asks for it.
    ///
    /// `WebDriver::managed` builds a *new* manager per call, so each session
    /// prepares the driver for itself. That is harmless when it is already
    /// cached and pathological when it is not: several sessions opening at once
    /// on a cold cache all try to download the same driver and contend on its
    /// lock file, which is a stall, not a slowdown. CI has a cold cache every
    /// run, which is exactly where the scenarios run in parallel.
    ///
    /// One session opened and closed up front settles it — the download happens
    /// once, and every later session finds the driver in place.
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

    /// Pins the browser's timezone, standing in for Playwright's
    /// `browser.newContext({ timezoneId })`.
    ///
    /// The old step needed a whole fresh context because `timezoneId` is fixed
    /// at context creation; CDP can change it on a live session, so the
    /// timezone scenario runs in the same browser as everything else.
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
    /// Handed to `reqwest` by [`crate::http`] so a status-code assertion is
    /// made as the logged-in user rather than as a stranger. `Ok(None)` when
    /// there is no cookie, which is not an error — a guest has none.
    ///
    /// # Errors
    ///
    /// Fails only on a driver error.
    pub async fn session_cookie(&self) -> Result<Option<String>> {
        // `LIFTLOG_COOKIE_SECURE` is off over plain HTTP, so the plain name is
        // the one in play; the `__Host-` variant only appears behind TLS.
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
