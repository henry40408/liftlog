//! CSRF audit logging around [`tower_http::csrf::CsrfLayer`], which (with the
//! session cookie's `SameSite=Lax`) is the whole CSRF defence. The layer's
//! rejection builder never sees the request, so this module pairs the captured
//! request with the [`ProtectionError`] on the 403 and logs `csrf.rejected`.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, header},
    middleware::Next,
    response::Response,
};
use tower_http::csrf::{ProtectionError, ProtectionErrorKind};

use crate::audit::AuditContext;

/// Client-IP attribution inputs; not `SessionLayerState`, as this runs outside
/// session validation.
#[derive(Clone)]
pub struct CsrfLayerState {
    pub trusted_proxy_header: crate::config::TrustedProxyHeader,
    pub trusted_proxies: std::sync::Arc<Vec<std::net::IpAddr>>,
}

/// Captured on the way in: `CsrfLayer` consumes the request it rejects.
struct PendingAudit {
    method: Method,
    path: String,
    headers: HeaderMap,
    extensions: axum::http::Extensions,
}

/// Logs every `CsrfLayer` rejection; `CsrfLayer` must be layered directly
/// inside this. The 403 stays bodyless: naming the failed check only helps
/// someone probing the guard.
pub async fn log_csrf_rejection(
    State(layer): State<CsrfLayerState>,
    req: Request,
    next: Next,
) -> Response {
    // Only capture for methods the layer can reject, sparing page loads the clone.
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

/// Which check fired; `origin_fallback` usually means an old browser or a proxy
/// rewriting `Host`.
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

/// Methods `CsrfLayer` never rejects. Not [`Method::is_safe`], which includes
/// `TRACE` — the layer checks it, so it must be captured.
fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::{Layer, ServiceExt, service_fn};
    use tower_http::csrf::CsrfLayer;

    /// `is_safe` must match the layer's exempt set exactly.
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
