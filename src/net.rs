use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;

use axum::http::HeaderMap;

use crate::config::TrustedProxyHeader;

/// Resolves the client IP for per-IP rate limiting.
///
/// Only the header named by `LIFTLOG_TRUSTED_PROXY_HEADER` is ever read —
/// presence proves nothing, since nginx forwards a client-forged
/// `X-Forwarded-For` verbatim. `None` means the TCP peer, always.
///
/// The header is honoured only when the peer is loopback or listed in
/// `trusted_proxies`; a missing peer (no `ConnectInfo`) is untrusted.
///
/// `X-Forwarded-For`: reads the *last* field line (appending proxies put the
/// trusted hop there) and strictly its *last* hop; anything leftward is
/// attacker-controlled, so an unparseable last hop falls back to the peer
/// rather than scanning left. Assumes exactly one trusted proxy. Hops may
/// carry a port (`ip:port`, `[v6]:port`). No CIDR support.
///
/// All IPs are canonicalized: a dual-stack listener sees IPv4 peers as
/// `::ffff:a.b.c.d`, which would otherwise fail the trust check and collapse
/// every client into one bucket.
pub fn client_ip(
    peer: Option<IpAddr>,
    headers: &HeaderMap,
    header: TrustedProxyHeader,
    trusted_proxies: &[IpAddr],
) -> IpAddr {
    let peer = peer.map(|p| p.to_canonical());
    let fallback = peer.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

    let trusted = peer
        .is_some_and(|p| p.is_loopback() || trusted_proxies.iter().any(|t| t.to_canonical() == p));

    match header {
        TrustedProxyHeader::XForwardedFor if trusted => {
            forwarded_for_last_hop(headers).unwrap_or(fallback)
        }
        TrustedProxyHeader::XRealIp if trusted => x_real_ip(headers).unwrap_or(fallback),
        TrustedProxyHeader::None
        | TrustedProxyHeader::XForwardedFor
        | TrustedProxyHeader::XRealIp => fallback,
    }
}

/// Accepts `ip`, `ip:port`, `[v6]:port` or `[v6]`; trailing garbage after `]`
/// is rejected, not truncated.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    let hop = hop.trim();
    if let Ok(ip) = IpAddr::from_str(hop) {
        return Some(ip);
    }
    if let Ok(addr) = SocketAddr::from_str(hop) {
        return Some(addr.ip());
    }
    hop.strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|bracketed| IpAddr::from_str(bracketed).ok())
}

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
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, peer);
    }

    #[test]
    fn missing_peer_ignores_headers() {
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(None, &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn loopback_peer_headers_are_honoured() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn listed_trusted_proxy_headers_are_honoured() {
        let peer: IpAddr = "10.0.0.5".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(
            Some(peer),
            &headers,
            TrustedProxyHeader::XForwardedFor,
            &[peer],
        );
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn xff_takes_the_rightmost_hop() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "203.0.113.1, 198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn x_real_ip_is_honoured_when_configured() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-real-ip", "198.51.100.9")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XRealIp, &[]);
        assert_eq!(resolved, "198.51.100.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn unparseable_xff_falls_back_to_peer() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "not-an-ip")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, peer);
    }

    #[test]
    fn header_none_never_reads_any_header() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[
            ("x-forwarded-for", "198.51.100.7"),
            ("x-real-ip", "198.51.100.9"),
        ]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::None, &[]);
        assert_eq!(resolved, peer);
    }

    #[test]
    fn x_real_ip_ignored_when_configured_header_is_xff() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-real-ip", "198.51.100.9")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, peer);
    }

    #[test]
    fn xff_ignored_when_configured_header_is_x_real_ip() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XRealIp, &[]);
        assert_eq!(resolved, peer);
    }

    /// The expected IP differs from the peer so a fallback can't pass.
    #[test]
    fn duplicate_xff_lines_use_the_last_line() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let mut headers = HeaderMap::new();
        append_header(&mut headers, "x-forwarded-for", "9.9.9.9");
        append_header(&mut headers, "x-forwarded-for", "198.51.100.7");
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn duplicate_x_real_ip_lines_use_the_last_line() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let mut headers = HeaderMap::new();
        append_header(&mut headers, "x-real-ip", "9.9.9.9");
        append_header(&mut headers, "x-real-ip", "198.51.100.7");
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XRealIp, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn xff_hop_with_port_is_parsed() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "9.9.9.9, 203.0.113.5:41234")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "203.0.113.5".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn xff_hop_bracketed_ipv6_with_port_is_parsed() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "[2001:db8::1]:443")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "2001:db8::1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn malformed_bracketed_hop_is_rejected() {
        let peer: IpAddr = "127.0.0.1".parse().unwrap();

        let headers = headers_with(&[("x-forwarded-for", "[9.9.9.9]junk")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, peer, "[9.9.9.9]junk should not parse");

        let headers = headers_with(&[("x-forwarded-for", "[::1]:80:80")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, peer, "[::1]:80:80 should not parse");

        let headers = headers_with(&[("x-forwarded-for", "[::1]:")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, peer, "[::1]: should not parse");
    }

    #[test]
    fn mapped_ipv6_loopback_peer_is_trusted() {
        let peer: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn mapped_ipv6_peer_matches_a_listed_trusted_proxy() {
        let peer: IpAddr = "::ffff:10.0.0.5".parse().unwrap();
        let trusted: IpAddr = "10.0.0.5".parse().unwrap();
        let headers = headers_with(&[("x-forwarded-for", "198.51.100.7")]);
        let resolved = client_ip(
            Some(peer),
            &headers,
            TrustedProxyHeader::XForwardedFor,
            &[trusted],
        );
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn mapped_ipv6_peer_is_canonicalised_in_the_fallback() {
        let peer: IpAddr = "::ffff:203.0.113.9".parse().unwrap();
        let headers = HeaderMap::new();
        let resolved = client_ip(Some(peer), &headers, TrustedProxyHeader::XForwardedFor, &[]);
        assert_eq!(resolved, "203.0.113.9".parse::<IpAddr>().unwrap());
    }
}
