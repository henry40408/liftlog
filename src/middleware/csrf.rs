//! First-line — and only — CSRF defence: reject state-changing requests that a
//! browser reports, or reveals, to be cross-site. Header-only; no token, no
//! state. Combined with the session cookie's `SameSite=Lax`, this is liftlog's
//! CSRF defence; there is no synchronizer token.
//!
//! The check itself is [`tower_http::csrf::CsrfLayer`], which implements the
//! scheme Go 1.25 shipped as `http.CrossOriginProtection`:
//!
//! - **`Sec-Fetch-Site`** (sent by every current browser) is authoritative when
//!   present. Only `same-origin` and `none` — a direct navigation or a
//!   user-typed URL — are allowed; `cross-site`, `same-site`, and any value the
//!   layer does not know are rejected. `same-site` covers a sibling subdomain or
//!   another port on the same host, which `SameSite=Lax` still hands the session
//!   cookie, so allowing it would leave exactly the caller this guard exists to
//!   stop.
//! - **`Origin`** is the fallback for the rare browser that omits
//!   `Sec-Fetch-Site` (Safari before 16.4) and for a plain-HTTP LAN install,
//!   where fetch metadata never arrives because the origin is not
//!   potentially-trustworthy. Its authority — host *and* port — is compared
//!   byte-for-byte against the request's own (the request-target authority if
//!   present, else `Host`); a mismatch, an opaque `Origin: null`, a malformed
//!   `Origin`, or a missing `Host` is rejected. Scheme is ignored, so a
//!   TLS-terminating proxy whose forwarded `Host` carries no scheme still
//!   passes.
//! - **Neither header** means a non-browser client (`curl`, the integration-test
//!   harness). Those are not exposed to an ambient-cookie CSRF and pass through.
//!
//! Matching the port is what this module used to give up (issue #187) to
//! tolerate `proxy_set_header Host $host;` on a non-default port. That tolerance
//! was worth less than it cost: cookies ignore ports, so another service on the
//! same host — a second container, a dev server, anything an attacker can get a
//! page onto — passed as same-origin while carrying the victim's session, and
//! with no synchronizer token behind it nothing else would have caught it. The
//! operator-facing price is one line in the README: forward `Host` with the port
//! the browser sent (nginx's `$http_host`, not `$host`).
//!
//! What this module still owns is [`log_csrf_rejection`]: the layer answers with
//! a bare `403` and its rejection builder never sees the request, so without an
//! enclosing layer an operator whose proxy trips the guard has nothing to work
//! from.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, header},
    middleware::Next,
    response::Response,
};
use tower_http::csrf::{ProtectionError, ProtectionErrorKind};

use crate::audit::AuditContext;

/// What the guard needs to attribute a rejection to a client IP. Deliberately
/// not `SessionLayerState`: this layer runs outside session validation and has
/// no business holding a session repository.
#[derive(Clone)]
pub struct CsrfLayerState {
    pub trusted_proxy_header: crate::config::TrustedProxyHeader,
    pub trusted_proxies: std::sync::Arc<Vec<std::net::IpAddr>>,
}

/// The pieces of a request a rejection needs to log, captured on the way in
/// because [`tower_http::csrf::CsrfLayer`] consumes the request it rejects and
/// hands its rejection builder only a [`ProtectionError`].
struct PendingAudit {
    method: Method,
    path: String,
    headers: HeaderMap,
    extensions: axum::http::Extensions,
}

/// Log every rejection by the first-line CSRF guard, `tower_http`'s
/// `CsrfLayer`, which must be layered directly *inside* this one so its 403 —
/// and the [`ProtectionError`] it attaches to the response — passes through
/// here.
///
/// The guard's only other symptom is an unexplained `403`, and the deployment
/// shapes that produce one legitimately (a proxy rewriting `Host`) are
/// indistinguishable in an access log from an attack. The *response* stays
/// bodyless on purpose: an attacker's page cannot read it anyway, and naming the
/// failed check only helps someone probing the guard.
pub async fn log_csrf_rejection(
    State(layer): State<CsrfLayerState>,
    req: Request,
    next: Next,
) -> Response {
    // Only built for methods the guard can reject, so the overwhelming majority
    // of requests — page loads — don't pay for a `HeaderMap` clone they never
    // log. The `AuditContext` itself is still deferred to the reject path.
    let pending = (!is_safe(req.method())).then(|| PendingAudit {
        method: req.method().clone(),
        path: req.uri().path().to_owned(),
        headers: req.headers().clone(),
        extensions: req.extensions().clone(),
    });

    let res = next.run(req).await;

    let (Some(err), Some(pending)) = (res.extensions().get::<ProtectionError>(), pending) else {
        return res;
    };
    let ctx = AuditContext::from_request_pieces(
        &pending.extensions,
        &pending.headers,
        &pending.path,
        layer.trusted_proxy_header,
        &layer.trusted_proxies,
    );
    let origin = header_str(pending.headers.get(header::ORIGIN));
    crate::audit::csrf_rejected(
        &ctx,
        rejected_by(err.kind()),
        pending.method.as_str(),
        origin,
    );
    res
}

/// Which of the guard's two checks fired. The `Origin` fallback is the one only
/// an old browser — or a proxy that rewrites `Host` — can trip, so it is worth
/// telling apart in the log from the browser declaring the request cross-site
/// itself.
fn rejected_by(kind: ProtectionErrorKind) -> &'static str {
    match kind {
        ProtectionErrorKind::CrossOriginRequest => "sec_fetch_site",
        ProtectionErrorKind::CrossOriginRequestFromOldBrowser => "origin_fallback",
        // `ProtectionErrorKind` is `#[non_exhaustive]`.
        _ => "other",
    }
}

/// A header value as a string, or `None` when absent or not ASCII.
fn header_str(value: Option<&HeaderValue>) -> Option<&str> {
    value.and_then(|v| v.to_str().ok())
}

/// Whether `method` cannot change server state and so is one `CsrfLayer` never
/// rejects. Deliberately not [`Method::is_safe`], which also counts `TRACE`:
/// this must mirror the layer's own set or a rejection would go unlogged.
fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::{Layer, ServiceExt, service_fn};
    use tower_http::csrf::CsrfLayer;

    /// Pins the assumption `log_csrf_rejection` makes when it skips capturing:
    /// a method this returns `true` for is never rejected by the layer, so
    /// nothing is lost by not capturing it. `TRACE` is "safe" per RFC 7231 but
    /// *is* checked by the layer, hence the explicit set.
    #[tokio::test]
    async fn the_skipped_methods_are_exactly_the_ones_the_layer_never_rejects() {
        for (method, safe) in [
            (Method::GET, true),
            (Method::HEAD, true),
            (Method::OPTIONS, true),
            (Method::TRACE, false),
            (Method::POST, false),
            (Method::PUT, false),
            (Method::DELETE, false),
            (Method::PATCH, false),
        ] {
            assert_eq!(is_safe(&method), safe, "{method} classified wrongly");

            let svc = CsrfLayer::new().layer(service_fn(|_: Request| async {
                Ok::<_, std::convert::Infallible>(Response::new(Body::empty()))
            }));
            let req = Request::builder()
                .method(method.clone())
                .uri("/anything")
                .header("sec-fetch-site", "cross-site")
                .body(Body::empty())
                .unwrap();
            let res = svc.oneshot(req).await.unwrap();
            assert_eq!(
                res.extensions().get::<ProtectionError>().is_some(),
                !safe,
                "{method}: the layer's own exempt set must match `is_safe`"
            );
        }
    }

    #[test]
    fn each_rejection_kind_gets_its_own_reason() {
        assert_eq!(
            rejected_by(ProtectionErrorKind::CrossOriginRequest),
            "sec_fetch_site"
        );
        assert_eq!(
            rejected_by(ProtectionErrorKind::CrossOriginRequestFromOldBrowser),
            "origin_fallback"
        );
    }

    #[test]
    fn a_non_ascii_origin_is_dropped_rather_than_logged_as_garbage() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_bytes(b"\xff\xfe").unwrap(),
        );
        assert_eq!(header_str(headers.get(header::ORIGIN)), None);
        assert_eq!(header_str(None), None);
    }
}
