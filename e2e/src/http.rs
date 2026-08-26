//! Direct HTTP requests, for the two things a `WebDriver` cannot answer.
//!
//! **Status codes.** `page.goto()` handed the old steps a `Response` with a
//! `status()`, so "returns a 404" and "returns a 403" were assertions about
//! what the server actually said. WebDriver has no equivalent — it reports the
//! rendered document and nothing about the exchange that produced it — and
//! matching on the error page's wording would pass just as happily on a 200
//! that happened to render it. [`status`] re-issues the request with the
//! browser's own session cookie instead, so the assertion stays about the
//! status line.
//!
//! **Guests.** The share scenarios need a visitor with no session. Playwright
//! spun up a second browser context for that; here the request simply carries
//! no cookie. The shared workout page is server-rendered with no scripts of its
//! own, so the returned HTML is the whole of what a guest's browser would show.

use anyhow::{Context, Result};

use crate::server::url;

/// One response, reduced to what the steps assert on.
pub struct Response {
    /// The HTTP status of the final response, after any redirects.
    pub status: u16,
    pub body: String,
}

/// Requests `path` as the holder of `session`, or as a guest when it is `None`.
///
/// Redirects are followed, matching `page.goto()`: a route that 302s to the
/// login page reports the login page's 200, which is what the old assertions
/// compared against.
///
/// # Errors
///
/// Fails when the request cannot be made or the body is not valid UTF-8.
pub async fn get(path: &str, session: Option<&str>) -> Result<Response> {
    let client = reqwest::Client::builder()
        .build()
        .context("building the HTTP client")?;

    let mut request = client.get(url(path));
    if let Some(token) = session {
        request = request.header(reqwest::header::COOKIE, format!("session={token}"));
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("requesting {path}"))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .with_context(|| format!("reading the body of {path}"))?;

    Ok(Response { status, body })
}

/// The status `path` answers with for the holder of `session`.
///
/// # Errors
///
/// Fails when the request cannot be made.
pub async fn status(path: &str, session: Option<&str>) -> Result<u16> {
    Ok(get(path, session).await?.status)
}
