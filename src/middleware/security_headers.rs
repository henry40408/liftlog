//! Security response headers: opt-in HSTS and an unconditional baseline.
//!
//! HSTS defaults off: liftlog never terminates TLS, so it cannot know a request
//! arrived over HTTPS, and a cached HSTS promise cannot be withdrawn before
//! `max-age` expires. Operators should prefer setting it on their proxy. No
//! `preload` knob: it is effectively irreversible. Browsers ignore the header
//! over plain HTTP (RFC 6797 §7.2), so enabling it there is merely ineffective.

use axum::extract::State;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// Pre-rendered `Strict-Transport-Security` value; `None` means disabled.
#[derive(Clone)]
pub struct HstsHeader(Option<HeaderValue>);

impl HstsHeader {
    /// `max_age == 0` disables the header.
    #[must_use]
    pub fn new(max_age: u64, include_subdomains: bool) -> Self {
        if max_age == 0 {
            return Self(None);
        }
        let rendered = if include_subdomains {
            format!("max-age={max_age}; includeSubDomains")
        } else {
            format!("max-age={max_age}")
        };
        let value =
            HeaderValue::from_str(&rendered).expect("rendered HSTS value is valid header ASCII");
        Self(Some(value))
    }
}

/// Adds `Strict-Transport-Security` when enabled. Outermost layer, so it also
/// covers short-circuited responses (CSRF 403, auth 302).
pub async fn hsts_middleware(
    State(hsts): State<HstsHeader>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let mut response = next.run(req).await;
    if let Some(value) = &hsts.0 {
        response
            .headers_mut()
            .insert(axum::http::header::STRICT_TRANSPORT_SECURITY, value.clone());
    }
    response
}

/// Baseline headers on every response.
///
/// - `frame-ancestors 'none'` / `X-Frame-Options: DENY` block clickjacking,
///   which neither `SameSite=Lax` nor the CSRF guard stops: a click inside a
///   framed page is same-origin, and the confirm pages are one click away.
/// - The CSP carries only `frame-ancestors`; a `default-src` would need nonces
///   and `fonts.bunny.net`, and is not attempted here.
/// - `Referrer-Policy` pins the modern default so `/shared/{token}` cannot leak
///   its token via `Referer` on older browsers.
pub async fn baseline_headers_middleware(req: axum::extract::Request, next: Next) -> Response {
    const BASELINE: [(axum::http::HeaderName, HeaderValue); 4] = [
        (
            axum::http::header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        ),
        (
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ),
        (
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        (
            axum::http::header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ),
    ];

    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    for (name, value) in BASELINE {
        headers.insert(name, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(hsts: &HstsHeader) -> Option<&str> {
        hsts.0.as_ref().map(|v| v.to_str().unwrap())
    }

    #[test]
    fn hsts_header_none_when_max_age_zero() {
        assert_eq!(rendered(&HstsHeader::new(0, false)), None);
        assert_eq!(rendered(&HstsHeader::new(0, true)), None);
    }

    #[test]
    fn hsts_header_value_without_subdomains() {
        assert_eq!(
            rendered(&HstsHeader::new(31_536_000, false)),
            Some("max-age=31536000")
        );
    }

    #[test]
    fn hsts_header_value_with_subdomains() {
        assert_eq!(
            rendered(&HstsHeader::new(31_536_000, true)),
            Some("max-age=31536000; includeSubDomains")
        );
    }
}
