//! Direct HTTP requests for what WebDriver cannot see: status codes (re-issued
//! with the browser's session cookie) and guests (no cookie).

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
/// Redirects are followed, so a 302 to the login page reports its 200.
///
/// # Errors
///
/// Fails when the request or reading the body fails.
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
