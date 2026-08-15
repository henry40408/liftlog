//! Creating accounts over HTTP, ported from `tests/e2e/support/seeding.js`.
//!
//! Fixtures go through the real endpoints rather than straight into SQLite, so
//! a scenario's account is created exactly the way a real one is — password
//! policy, hashing, roles and all. The first account comes from `/auth/setup`
//! (which makes it the admin); everyone else is created by that admin through
//! `/users/new`.
//!
//! The whole sequence is serialised behind a mutex. Playwright gave each worker
//! its own server and database, so two workers could never race to create the
//! first user; cucumber runs its scenarios against one server, and without the
//! lock two of them arriving at `/auth/setup` together would have one of the
//! two see a half-created install.
//!
//! `csrf_origin_guard` lets these through: it is header-only, and rejects a
//! request only when the browser *reports* it as cross-site. A `reqwest` call
//! sends no `Sec-Fetch-Site` and no `Origin`, so it is treated as the
//! non-browser client it is.

use anyhow::{Context, Result, bail};
use tokio::sync::Mutex;

use crate::server::{ADMIN, PASSWORD, url};

/// Serialises account creation across concurrently running scenarios.
static SEEDING: Mutex<()> = Mutex::const_new(());

/// Makes sure `username` exists with `password`, creating it if it does not.
///
/// Idempotent, like the step it replaces: a repeat `/auth/setup` redirects to
/// the login page because a user already exists, and a repeat `/users/new`
/// re-renders its form with a "taken" error. Both are ignored — the
/// post-condition is that the account is there, not that this call is what made
/// it.
///
/// # Errors
///
/// Fails when a request cannot be made, or when `/auth/setup` answers with
/// something other than the form, a redirect, or a re-render.
pub async fn ensure_user(username: &str, password: &str) -> Result<()> {
    let _guard = SEEDING.lock().await;

    // Everyone needs the admin: it is either the account being asked for, or
    // the one that has to create it.
    call_setup().await?;
    if username == ADMIN {
        return Ok(());
    }

    let client = client()?;
    admin_login(&client).await?;
    client
        .post(url("/users/new"))
        .form(&[("username", username), ("password", password)])
        .send()
        .await
        .with_context(|| format!("creating the user {username}"))?;
    Ok(())
}

/// Signs in as `username` on a throwaway client, leaving a second live session
/// behind.
///
/// What `steps/settings.steps.js`'s `playwright.request.newContext()` did: the
/// session has to exist without becoming the browser's, so the active-sessions
/// table has two rows to show.
///
/// # Errors
///
/// Fails when the request cannot be made, or when the login does not redirect —
/// a re-rendered form means the credentials were refused.
pub async fn open_second_session(username: &str, password: &str) -> Result<()> {
    let response = client()?
        .post(url("/auth/login"))
        .form(&[("username", username), ("password", password)])
        .send()
        .await
        .with_context(|| format!("signing {username} in for a second session"))?;

    let status = response.status();
    if !status.is_redirection() {
        bail!("a second session for {username} expected a redirect, got {status}");
    }
    Ok(())
}

/// Creates the first user, or bounces off the install that already has one.
async fn call_setup() -> Result<()> {
    let response = client()?
        .post(url("/auth/setup"))
        .form(&[("username", ADMIN), ("password", PASSWORD)])
        .send()
        .await
        .context("calling /auth/setup")?;

    let status = response.status();
    if !(status.is_success() || status.is_redirection()) {
        bail!("/auth/setup answered {status}");
    }
    Ok(())
}

async fn admin_login(client: &reqwest::Client) -> Result<()> {
    let response = client
        .post(url("/auth/login"))
        .form(&[("username", ADMIN), ("password", PASSWORD)])
        .send()
        .await
        .context("signing the admin in")?;

    let status = response.status();
    if !status.is_redirection() {
        bail!("the admin login expected a redirect, got {status}");
    }
    Ok(())
}

/// A client that keeps its cookies and does not chase redirects.
///
/// Both matter: the admin's session has to survive from the login to the
/// `/users/new` post, and a followed redirect would turn the 302 that says
/// "signed in" into the 200 of the page it points at, hiding a refused login.
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("building the seeding HTTP client")
}
