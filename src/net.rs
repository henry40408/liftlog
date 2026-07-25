use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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
/// `X-Forwarded-For` is a repeatable header: `HeaderMap::get` returns only
/// the *first* field line, but proxies that **append** a new line rather
/// than merging into an existing one (`HAProxy`'s `option forwardfor`,
/// Caddy's `header_up +X-Forwarded-For`, anything built on Go's
/// `Header.Add`) put the trusted hop in the *last* line. Reading the first
/// line makes the "trusted" hop entirely attacker-chosen, since a client can
/// simply send its own `X-Forwarded-For` line ahead of the one the proxy
/// appends. This resolver therefore reads the last of
/// `headers.get_all(...)` (via `.iter().next_back()`, equivalent to `.last()`
/// but without needlessly walking a `DoubleEndedIterator` end to end) for
/// both `X-Forwarded-For` and `X-Real-IP`, and takes **strictly the
/// last hop** of that last line — nginx's stock `proxy_set_header
/// X-Forwarded-For $proxy_add_x_forwarded_for` and Caddy's `reverse_proxy`
/// both *append* the observed peer address within that line, so the
/// leftmost entries are attacker-controlled free-form text and only the
/// rightmost entry was actually written by the trusted proxy. If that last
/// hop does not parse as an IP, this **fails closed to the TCP peer** —
/// it does not scan leftwards for an earlier parseable hop, because every
/// step leftwards is a step toward attacker-controlled data. This logic
/// assumes exactly one trusted proxy hop in front of liftlog; a chain of
/// multiple proxies is not supported.
///
/// Each hop may be a bare IP, or (to tolerate load balancers such as Azure
/// Application Gateway that append a port) `ip:port` / `[v6]:port`.
///
/// If any `X-Forwarded-For` header line was present at all, `X-Real-IP` is
/// **never** consulted, even if the last XFF hop fails to parse — once the
/// trusted proxy has written XFF, `X-Real-IP` carries no additional trust,
/// and falling back to it would let an attacker mint an arbitrary client IP
/// simply by sending an unparseable XFF hop alongside a forged `X-Real-IP`.
/// `X-Real-IP` is only consulted when `X-Forwarded-For` is absent entirely.
///
/// Every IP — the peer and any IP read from a header — is passed through
/// [`IpAddr::to_canonical`] before use. Without this, an IPv4-mapped IPv6
/// peer address (`::ffff:127.0.0.1`) is not `is_loopback()` and does not
/// equal a `trusted_proxies` entry written in plain IPv4 form, even though
/// it is the same address. This is not a corner case: binding `[::]:PORT`
/// gives a dual-stack listener, so a same-host reverse proxy connecting over
/// IPv4 arrives mapped. Without canonicalization the trust check silently
/// fails and every client collapses into the single "untrusted" fallback
/// bucket, so one attacker's failed logins would lock out the entire user
/// base. Canonicalizing header-sourced IPs too ensures one physical client
/// always maps to one rate-limit bucket regardless of which family a proxy
/// happened to log it in.
///
/// Only bare IP addresses (optionally with a port, per above) are accepted —
/// no CIDR ranges. Supporting CIDR notation for `trusted_proxies` would
/// require a new dependency, which is undesirable given the project's 7-day
/// dependency-cooldown policy; it is a possible follow-up.
pub fn client_ip(peer: Option<IpAddr>, headers: &HeaderMap, trusted_proxies: &[IpAddr]) -> IpAddr {
    let peer = peer.map(|p| p.to_canonical());
    let fallback = peer.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let trusted = peer
        .is_some_and(|p| p.is_loopback() || trusted_proxies.iter().any(|t| t.to_canonical() == p));

    if !trusted {
        return fallback;
    }

    if headers.contains_key("x-forwarded-for") {
        return forwarded_for_last_hop(headers).unwrap_or(fallback);
    }

    x_real_ip(headers).unwrap_or(fallback)
}

/// Parses one forwarding hop as an IP address, tolerating a trailing port:
/// a bare `IpAddr`, `ip:port` / `[v6]:port` (as `SocketAddr`), or a bracketed
/// `[v6]` with no port.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    let hop = hop.trim();
    if let Ok(ip) = IpAddr::from_str(hop) {
        return Some(ip);
    }
    if let Ok(addr) = SocketAddr::from_str(hop) {
        return Some(addr.ip());
    }
    if let Some(rest) = hop.strip_prefix('[') {
        let bracketed = rest.split(']').next()?;
        return IpAddr::from_str(bracketed).ok();
    }
    None
}

/// The last hop of the *last* `X-Forwarded-For` header field line, or `None`
/// if there is no such header, it isn't valid UTF-8, or that last hop
/// doesn't parse. Deliberately does not consider earlier header lines or
/// earlier hops within the last line — see the `client_ip` doc comment.
fn forwarded_for_last_hop(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()?
        .to_str()
        .ok()?;
    let last_hop = raw.rsplit(',').next()?;
    parse_hop(last_hop).map(|ip| ip.to_canonical())
}

/// The value of the last `X-Real-IP` header field line, or `None` if there
/// is no such header, it isn't valid UTF-8, or it doesn't parse.
fn x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers
        .get_all("x-real-ip")
        .iter()
        .next_back()?
        .to_str()
        .ok()?;
    parse_hop(raw).map(|ip| ip.to_canonical())
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

    fn append_header(headers: &mut HeaderMap, name: &str, value: &str) {
        headers.append(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
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

    /// Regression test for the confirmed bypass: two separate
    /// `X-Forwarded-For` field lines (as `HeaderMap::append` produces, the
    /// way HAProxy/Caddy/Go proxies emit them) must be resolved from the
    /// *last* line, not the first. Reading the first line lets an attacker
    /// pick any IP by varying the header they send while the trusted proxy's
    /// appended line sits second.
    #[test]
    fn duplicate_xff_lines_use_the_last_line() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let mut headers = HeaderMap::new();
        append_header(&mut headers, "x-forwarded-for", "9.9.9.9");
        append_header(&mut headers, "x-forwarded-for", "127.0.0.1");
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn duplicate_x_real_ip_lines_use_the_last_line() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let mut headers = HeaderMap::new();
        append_header(&mut headers, "x-real-ip", "9.9.9.9");
        append_header(&mut headers, "x-real-ip", "198.51.100.7");
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn xff_hop_with_port_is_parsed() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "9.9.9.9, 203.0.113.5:41234")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "203.0.113.5".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn xff_hop_bracketed_ipv6_with_port_is_parsed() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "[2001:db8::1]:443")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "2001:db8::1".parse::<IpAddr>().unwrap());
    }

    /// Regression test: an unparseable last XFF hop must fail closed to the
    /// peer, never escalate to `X-Real-IP`, even when that header is
    /// present. Confirmed exploit: XFF "9.9.9.9, 203.0.113.5:41234" (malformed
    /// variants) + a forged X-Real-IP resolved to the forged header.
    #[test]
    fn unparseable_last_hop_does_not_escalate_to_x_real_ip() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[
            ("x-forwarded-for", "9.9.9.9, garbage"),
            ("x-real-ip", "8.8.8.8"),
        ]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, peer);
    }

    #[test]
    fn mapped_ipv6_loopback_peer_is_trusted() {
        let peer: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn mapped_ipv6_peer_matches_a_listed_trusted_proxy() {
        let peer: IpAddr = "::ffff:10.0.0.5".parse().unwrap();
        let trusted: IpAddr = "10.0.0.5".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, &[trusted]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn mapped_ipv6_peer_is_canonicalised_in_the_fallback() {
        let peer: IpAddr = "::ffff:203.0.113.9".parse().unwrap();
        let headers = HeaderMap::new();
        let resolved = client_ip(Some(peer), &headers, &[]);
        assert_eq!(resolved, "203.0.113.9".parse::<IpAddr>().unwrap());
    }
}
