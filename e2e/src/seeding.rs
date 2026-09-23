//! Creating accounts over the real HTTP endpoints, so policy, hashing and roles
//! apply. Serialised behind a mutex: concurrent scenarios hitting
//! `/auth/setup` together would race on the first user. The CSRF guard lets
//! these through because `reqwest` sends no `Sec-Fetch-Site` or `Origin`.

use anyhow::{Context, Result, bail};
use tokio::sync::Mutex;

use crate::server::{ADMIN, PASSWORD, url};

/// Serialises account creation across concurrently running scenarios.
static SEEDING: Mutex<()> = Mutex::const_new(());

/// Makes sure `username` exists, creating it if not. Idempotent: a repeat
/// `/auth/setup` redirects and a repeat `/users/new` re-renders "taken"; both
/// are ignored.
///
/// # Errors
///
/// Fails when a request cannot be made, or when `/auth/setup` answers with
/// something other than the form, a redirect, or a re-render.
pub async fn ensure_user(username: &str, password: &str) -> Result<()> {
    let _guard = SEEDING.lock().await;

    // The admin is either the account asked for or the one that creates it.
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
/// that is not the browser's.
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

/// Keeps cookies (the admin session spans login → `/users/new`) and does not
/// follow redirects (the login's 302 is how success is detected).
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("building the seeding HTTP client")
}
