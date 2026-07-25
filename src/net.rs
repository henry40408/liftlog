use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use axum::http::HeaderMap;

/// Resolves the client IP to use for per-IP rate limiting.
///
/// Proxy headers (`X-Forwarded-For`, `X-Real-IP`) are only honoured when the
/// TCP peer itself is trustworthy: either `peer` is a loopback address, or it
/// appears in `trusted_proxies`. A **missing** peer (`peer` is `None`, as
/// happens when a caller has no `ConnectInfo`, e.g. integration tests driving
/// the router directly with `oneshot`) is treated as untrusted and falls
/// straight through to the final fallback — this fails closed rather than
/// open.
///
/// When headers are trusted and `X-Forwarded-For` is present, the
/// **rightmost** (last) hop is used, not the leftmost. This is easy to get
/// backwards: nginx's stock `proxy_set_header X-Forwarded-For
/// $proxy_add_x_forwarded_for` and Caddy's `reverse_proxy` both *append* the
/// observed peer address to whatever the client already sent, so the
/// leftmost entry is attacker-controlled free-form text and only the
/// rightmost entry was actually written by the trusted proxy. Reading the
/// leftmost value would make the rate limit trivially bypassable by varying
/// a request header. This logic assumes exactly one trusted proxy hop in
/// front of liftlog; a chain of multiple proxies is not supported.
///
/// If `X-Forwarded-For` is absent or none of its hops parse as a bare IP,
/// falls back to `X-Real-IP` (a single value), then to `peer`, then finally
/// to `127.0.0.1`.
///
/// Only bare IP addresses are accepted — no CIDR ranges. Supporting CIDR
/// notation for `trusted_proxies` would require a new dependency, which is
/// undesirable given the project's 7-day dependency-cooldown policy; it is a
/// possible follow-up.
pub fn client_ip(peer: Option<IpAddr>, headers: &HeaderMap, trusted_proxies: &[IpAddr]) -> IpAddr {
    let peer_is_trusted = peer.is_some_and(|p| p.is_loopback() || trusted_proxies.contains(&p));

    if peer_is_trusted {
        if let Some(ip) = rightmost_forwarded_for(headers) {
            return ip;
        }
        if let Some(ip) = x_real_ip(headers) {
            return ip;
        }
    }

    peer.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

fn rightmost_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers.get("x-forwarded-for")?.to_str().ok()?;
    let last = raw.rsplit(',').next()?.trim();
    IpAddr::from_str(last).ok()
}

fn x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers.get("x-real-ip")?.to_str().ok()?;
    IpAddr::from_str(raw.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    #[test]
    fn untrusted_peer_headers_are_ignored() {
        let peer: IpAddr = "203.0.113.9".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, peer);
    }

    #[test]
    fn missing_peer_ignores_headers() {
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(None, &headers, &[]);
        assert_eq!(resolved, "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn loopback_peer_headers_are_honoured() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn listed_trusted_proxy_headers_are_honoured() {
        let peer: IpAddr = "10.0.0.5".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, &[peer]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn xff_takes_the_rightmost_hop() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "203.0.113.1, 198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn falls_back_to_x_real_ip() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-real-ip", "198.51.100.9")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "198.51.100.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn unparseable_xff_falls_back_to_peer() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "not-an-ip")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, peer);
    }
}
