//! `Strict-Transport-Security` (HSTS), per the [OWASP HTTP Strict Transport
//! Security Cheat
//! Sheet](https://cheatsheetseries.owasp.org/cheatsheets/HTTP_Strict_Transport_Security_Cheat_Sheet.html).
//!
//! Defaults off. liftlog never terminates TLS — it just binds a TCP listener
//! (see `src/main.rs`) and expects a reverse proxy in front of it — so it has
//! no way to know whether a given request really arrived over HTTPS; it can
//! only trust the operator's configuration. HSTS is a promise a browser
//! caches and enforces for `max-age` seconds, and that promise cannot be
//! withdrawn from the server side once a browser has it: there is no
//! "un-send", only waiting out the `max-age`. That makes a wrong HSTS
//! deployment a uniquely bad failure mode for a self-hosted app, so the
//! header is opt-in, and the README steers operators toward setting it on
//! their TLS-terminating reverse proxy instead, where it belongs.
//!
//! There is deliberately no `preload` knob here. `preload` requires
//! `includeSubDomains` plus `max-age >= 31536000` and submission to a
//! browser-vendor list, and is effectively irreversible once accepted —
//! nowhere near something that should be reachable from an env var. Anyone
//! who wants it can configure it on their reverse proxy.
//!
//! Per RFC 6797 §7.2, a user agent MUST ignore any `Strict-Transport-Security`
//! header received over a non-secure transport (plain HTTP). So enabling
//! `LIFTLOG_HSTS_MAX_AGE` on a deployment that is not actually served over HTTPS is
//! merely ineffective, not harmful in itself — but operators must not read
//! "I set `LIFTLOG_HSTS_MAX_AGE`" as "my site is secure": the header only does
//! anything once HTTPS is genuinely in place end to end.

use axum::extract::State;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// Pre-rendered `Strict-Transport-Security` value; `None` means disabled.
#[derive(Clone)]
pub struct HstsHeader(Option<HeaderValue>);

impl HstsHeader {
    /// `max_age == 0` disables the header entirely. The value is rendered
    /// once here rather than per request, since it never changes for the
    /// lifetime of the process.
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

/// Appends `Strict-Transport-Security` to every response when enabled.
///
/// Registered as the outermost layer in `create_router` so it also reaches
/// responses that short-circuit inside inner layers — e.g. `csrf_origin_guard`'s
/// `403` and the sliding-session middleware's `AuthRedirect` `302` — not just
/// ones that make it all the way to a handler.
pub async fn hsts_middleware(
    State(hsts): State<HstsHeader>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let mut response = next.run(req).await;
    if let Some(value) = &hsts.0 {
        // `insert`, not `append`: liftlog must contribute at most one
        // Strict-Transport-Security header.
        response
            .headers_mut()
            .insert(axum::http::header::STRICT_TRANSPORT_SECURITY, value.clone());
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
