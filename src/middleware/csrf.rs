//! First-line CSRF defence: reject state-changing requests that a browser
//! reports, or reveals, to be cross-site. Header-only; no token, no state.
//! Combined with the session cookie's `SameSite=Lax`, this is liftlog's CSRF
//! defence. See `rdrs/src/middleware/csrf.rs` for the original.
//!
//! - **`Sec-Fetch-Site`** (sent by every current browser) is authoritative when
//!   present. `same-origin`, `same-site`, and `none` (a direct navigation or a
//!   user-typed URL) are allowed; only `cross-site` is rejected.
//! - **`Origin`** is the fallback for the rare browser that omits
//!   `Sec-Fetch-Site`. Its host is compared against the request's own `Host`;
//!   a mismatch — or an opaque `Origin: null` — is rejected.
//! - **Neither header** means a non-browser client (`curl`, the integration-test
//!   harness). Those are not exposed to an ambient-cookie CSRF and pass through.
//!
//! Scheme and port are deliberately ignored in the `Origin`/`Host` comparison:
//! behind a TLS-terminating reverse proxy the browser's `Origin` is `https://`
//! while the forwarded `Host` carries no scheme and often no port. Matching on
//! host alone keeps the check working in that standard deployment without a
//! configured public URL.
//!
//! That tolerance is load-bearing rather than incidental, which is why this
//! module is hand-rolled instead of delegating to `tower_http::csrf` (issue
//! #187). Fetch metadata is only sent to potentially-trustworthy origins, so on
//! a plain-HTTP LAN install — a deployment shape the README supports
//! first-class — no `Sec-Fetch-Site` ever arrives and this comparison is the
//! *only* check running, not a fallback. An upstream byte-exact authority match
//! would then 403 every POST, login included, behind the widely-copied
//! `proxy_set_header Host $host;` on a non-default port.
//!
//! Every rejection is logged as a `csrf.rejected` audit event naming which
//! branch rejected it — the failure this guard produces is an unexplained
//! `403`, and without the event an operator whose proxy trips it has nothing
//! to work from.

use axum::{
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::audit::AuditContext;

/// What the guard needs to attribute a rejection to a client IP. Deliberately
/// not `SessionLayerState`: this layer runs outside session validation and has
/// no business holding a session repository.
#[derive(Clone)]
pub struct CsrfLayerState {
    pub trusted_proxy_header: crate::config::TrustedProxyHeader,
    pub trusted_proxies: std::sync::Arc<Vec<std::net::IpAddr>>,
}

/// Which branch judged a request cross-site. Recorded on the audit event
/// because the branches differ in what they prove: [`Self::SecFetchSite`] is
/// the browser itself declaring the request cross-site, while the `Origin`
/// reasons are inferred from headers a reverse proxy may have rewritten. An
/// operator seeing the latter should suspect their proxy first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrossSiteReason {
    /// `Sec-Fetch-Site: cross-site` — the browser said so.
    SecFetchSite,
    /// `Origin: null`, opaque (a sandboxed iframe, a cross-origin redirect).
    OriginOpaque,
    /// An `Origin` with no readable `scheme://host` authority.
    OriginMalformed,
    /// `Origin` present but `Host` absent or unreadable — cannot be confirmed
    /// same-origin. Overwhelmingly a proxy that dropped the header.
    HostMissing,
    /// `Origin`'s host and the request's `Host` name different hosts.
    OriginHostMismatch,
}

impl CrossSiteReason {
    /// Snake-case wire form for the `reason` field, matching `session.rejected`.
    fn as_str(self) -> &'static str {
        match self {
            Self::SecFetchSite => "sec_fetch_site",
            Self::OriginOpaque => "origin_opaque",
            Self::OriginMalformed => "origin_malformed",
            Self::HostMissing => "host_missing",
            Self::OriginHostMismatch => "origin_host_mismatch",
        }
    }
}

/// Reject a state-changing request that is provably cross-site. Safe methods
/// (GET/HEAD/OPTIONS/TRACE) pass through untouched; a request that carries
/// neither `Sec-Fetch-Site` nor `Origin` is treated as a non-browser client
/// (curl, the integration-test harness) and allowed.
pub async fn csrf_origin_guard(
    State(layer): State<CsrfLayerState>,
    req: Request,
    next: Next,
) -> Response {
    if is_safe(req.method()) {
        return next.run(req).await;
    }
    let Some(reason) = cross_site_reason(&req) else {
        return next.run(req).await;
    };

    // Built only on the reject path: the overwhelming majority of requests are
    // same-origin and shouldn't pay for client-IP resolution they never log.
    let ctx = AuditContext::from_request_pieces(
        req.extensions(),
        req.headers(),
        req.uri().path(),
        layer.trusted_proxy_header,
        &layer.trusted_proxies,
    );
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    crate::audit::csrf_rejected(&ctx, reason.as_str(), req.method().as_str(), origin);

    StatusCode::FORBIDDEN.into_response()
}

/// Whether `method` cannot change server state and so needs no CSRF check.
fn is_safe(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

/// Why the request is one a browser has told us — via `Sec-Fetch-Site` or a
/// mismatched `Origin` — is cross-site, or `None` when it is not. A request a
/// browser did not mark, and that carries no `Origin`, is treated as
/// not-cross-site (a non-browser client); see the module docs.
fn cross_site_reason(req: &Request) -> Option<CrossSiteReason> {
    let headers = req.headers();

    // `Sec-Fetch-Site` is authoritative where the browser sends it.
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        return site
            .eq_ignore_ascii_case("cross-site")
            .then_some(CrossSiteReason::SecFetchSite);
    }

    // Fall back to comparing the Origin's host with the request's own Host.
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())?;
    // `Origin: null` is opaque (a sandboxed iframe, a cross-origin redirect) and
    // never legitimate for a state-changing request here.
    if origin.eq_ignore_ascii_case("null") {
        return Some(CrossSiteReason::OriginOpaque);
    }
    let Some(origin_host) = host_of(origin) else {
        return Some(CrossSiteReason::OriginMalformed);
    };
    // A missing/garbled Host with a present Origin cannot be confirmed
    // same-origin, so treat it as cross-site.
    let Some(request_host) = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(strip_port)
    else {
        return Some(CrossSiteReason::HostMissing);
    };

    (request_host != origin_host).then_some(CrossSiteReason::OriginHostMismatch)
}

/// The host of an `Origin` value (`scheme://host[:port]`), lower-cased and with
/// any port removed. `None` when there is no `://` authority to read.
fn host_of(origin: &str) -> Option<String> {
    let authority = origin.split_once("://").map(|(_, rest)| rest)?;
    Some(strip_port(authority).to_ascii_lowercase())
}

/// Strip a trailing `:port` from a host authority, leaving the host. Handles
/// bracketed IPv6 literals (`[::1]:8080` → `[::1]`).
fn strip_port(authority: &str) -> String {
    if let Some(end) = authority
        .strip_prefix('[')
        .and_then(|_| authority.find(']'))
    {
        // Bracketed IPv6: keep through the closing bracket, drop any `:port`.
        return authority[..=end].to_ascii_lowercase();
    }
    authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn req(method: Method, headers: &[(&str, &str)]) -> Request {
        let mut b = Request::builder().method(method).uri("/anything");
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    }

    fn is_cross_site(req: &Request) -> bool {
        cross_site_reason(req).is_some()
    }

    #[test]
    fn safe_methods_are_never_cross_site_checked() {
        // Even an obviously cross-site GET passes — GET must not change state.
        let r = req(Method::GET, &[("sec-fetch-site", "cross-site")]);
        assert!(is_safe(r.method()));
    }

    #[test]
    fn sec_fetch_site_is_authoritative() {
        for allowed in ["same-origin", "same-site", "none", "SAME-ORIGIN"] {
            assert!(
                !is_cross_site(&req(Method::POST, &[("sec-fetch-site", allowed)])),
                "{allowed} must be allowed"
            );
        }
        assert_eq!(
            cross_site_reason(&req(Method::POST, &[("sec-fetch-site", "cross-site")])),
            Some(CrossSiteReason::SecFetchSite)
        );
        // It wins over a same-looking Origin/Host, in both directions.
        assert_eq!(
            cross_site_reason(&req(
                Method::POST,
                &[
                    ("sec-fetch-site", "cross-site"),
                    ("origin", "https://app.example.com"),
                    ("host", "app.example.com"),
                ]
            )),
            Some(CrossSiteReason::SecFetchSite)
        );
    }

    #[test]
    fn origin_fallback_compares_host_ignoring_scheme_and_port() {
        // TLS-terminating proxy: Origin is https://, Host has no scheme/port.
        assert!(!is_cross_site(&req(
            Method::POST,
            &[
                ("origin", "https://app.example.com"),
                ("host", "app.example.com"),
            ]
        )));
        // Port on the Origin, none on Host → still same host.
        assert!(!is_cross_site(&req(
            Method::POST,
            &[("origin", "http://localhost:8080"), ("host", "localhost"),]
        )));
        // Genuine cross-origin.
        assert_eq!(
            cross_site_reason(&req(
                Method::POST,
                &[
                    ("origin", "https://evil.example.com"),
                    ("host", "app.example.com"),
                ]
            )),
            Some(CrossSiteReason::OriginHostMismatch)
        );
        // Opaque origin.
        assert_eq!(
            cross_site_reason(&req(
                Method::POST,
                &[("origin", "null"), ("host", "app.example.com")]
            )),
            Some(CrossSiteReason::OriginOpaque)
        );
    }

    #[test]
    fn ipv6_literal_host_is_compared_without_its_port() {
        assert!(!is_cross_site(&req(
            Method::POST,
            &[("origin", "http://[::1]:8080"), ("host", "[::1]")]
        )));
    }

    #[test]
    fn non_browser_client_without_headers_passes() {
        // curl / the integration-test harness sends neither header and does not
        // ride an ambient session cookie the way a forged browser POST would, so
        // it is not a CSRF vector.
        assert!(!is_cross_site(&req(Method::POST, &[])));
    }

    /// The two ways the `Origin` branch fails short of an actual host
    /// comparison. Distinguished on the audit event because a stripped `Host`
    /// points at the reverse proxy, not at an attacker.
    #[test]
    fn unusable_origin_and_missing_host_are_reported_apart() {
        assert_eq!(
            cross_site_reason(&req(
                Method::POST,
                &[("origin", "not-an-origin"), ("host", "app.example.com")]
            )),
            Some(CrossSiteReason::OriginMalformed)
        );
        assert_eq!(
            cross_site_reason(&req(Method::POST, &[("origin", "https://app.example.com")])),
            Some(CrossSiteReason::HostMissing)
        );
    }
}
