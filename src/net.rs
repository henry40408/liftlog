use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;

use axum::http::HeaderMap;

use crate::config::TrustedProxyHeader;

/// Resolves the client IP to use for per-IP rate limiting.
///
/// Which forwarding header (if any) may be trusted is an explicit operator
/// choice (`header`, from `TRUSTED_PROXY_HEADER`), never inferred from
/// which headers happen to be present. The mere presence of an
/// `X-Forwarded-For` line is not proof a trusted proxy wrote it: nginx
/// forwards a client-supplied `X-Forwarded-For` upstream verbatim unless the
/// config explicitly overwrites it, so a minimal reverse-proxy config that
/// only sets `X-Real-IP` would let an attacker's forged `X-Forwarded-For`
/// outrank the proxy-written `X-Real-IP` if either header were trusted just
/// because it showed up. Choosing the header up front closes that gap and
/// also gives operators a real opt-out: `TrustedProxyHeader::None` means no
/// header is ever read, regardless of peer.
///
/// If `header` is [`TrustedProxyHeader::None`], the TCP peer is used
/// unconditionally and no header is read at all.
///
/// Otherwise, the configured header is only honoured when the TCP peer
/// itself is trustworthy: either `peer` is a loopback address, or it appears
/// in `trusted_proxies`. A **missing** peer (`peer` is `None`, as happens
/// when a caller has no `ConnectInfo`, e.g. integration tests driving the
/// router directly with `oneshot`) is treated as untrusted and falls
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
/// but without needlessly walking a `DoubleEndedIterator` end to end), and
/// (for `X-Forwarded-For`) takes **strictly the last hop** of that last
/// line — nginx's stock `proxy_set_header X-Forwarded-For
/// $proxy_add_x_forwarded_for` and Caddy's `reverse_proxy` both *append*
/// the observed peer address within that line, so the leftmost entries are
/// attacker-controlled free-form text and only the rightmost entry was
/// actually written by the trusted proxy. If that last hop does not parse
/// as an IP, this **fails closed to the TCP peer** — it does not scan
/// leftwards for an earlier parseable hop, because every step leftwards is
/// a step toward attacker-controlled data. This logic assumes exactly one
/// trusted proxy hop in front of liftlog; a chain of multiple proxies is
/// not supported.
///
/// Each hop may be a bare IP, or (to tolerate load balancers such as Azure
/// Application Gateway that append a port) `ip:port` / `[v6]:port`.
///
/// Only the configured header is ever read — the other one is never
/// consulted under any circumstance, even if the configured header is
/// present but fails to parse.
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
/// happened to log it in. (`Config` already canonicalizes
/// `trusted_proxies` at parse time; the `.to_canonical()` call on each
/// entry here is defence-in-depth for callers constructing the slice
/// directly rather than through `Config`.)
///
/// Only bare IP addresses (optionally with a port, per above) are accepted —
/// no CIDR ranges. Supporting CIDR notation for `trusted_proxies` would
/// require a new dependency, which is undesirable given the project's 7-day
/// dependency-cooldown policy; it is a possible follow-up.
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

/// Parses one forwarding hop as an IP address, tolerating a trailing port:
/// a bare `IpAddr`, `ip:port` / `[v6]:port` (as `SocketAddr`), or a bracketed
/// `[v6]` with no port. The bracket form requires the closing `]` to end the
/// string — trailing garbage after it (`[::1]extra`, `[::1]:80:80`) is
/// rejected rather than silently truncated, since bare IPs and `ip:port` /
/// `[v6]:port` are already fully covered by the two preceding branches.
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

    /// Opt-out test: with `TrustedProxyHeader::None`, no header is ever
    /// read, even from a trusted (loopback) peer with a header present.
    /// This is the missing opt-out from the confirmed bypass: an operator
    /// whose proxy does not sanitise forwarding headers can leave
    /// `TRUSTED_PROXY_HEADER` unset and get the safe TCP-peer behaviour.
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

    /// The non-selected header must never be consulted, even when it is the
    /// only one present. This is the core of the fix: which header is
    /// authoritative is configured, not inferred from presence.
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

    /// Regression test for the confirmed bypass: two separate
    /// `X-Forwarded-For` field lines (as `HeaderMap::append` produces, the
    /// way HAProxy/Caddy/Go proxies emit them) must be resolved from the
    /// *last* line, not the first. Reading the first line lets an attacker
    /// pick any IP by varying the header they send while the trusted proxy's
    /// appended line sits second.
    ///
    /// The last line's value is deliberately distinct from the loopback
    /// peer (`198.51.100.7`, not `127.0.0.1`): using the peer's own address
    /// as the expected result would make "resolved from the last line" and
    /// "header ignored, fell back to the peer" indistinguishable — a prior
    /// version of this test passed even with the XFF branch replaced by
    /// `return fallback;`.
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

    /// `parse_hop`'s bracket branch must require the closing `]` to end the
    /// string. `rest.split(']').next()` would accept trailing garbage after
    /// the bracket; verified here (through `client_ip`, since `parse_hop` is
    /// private) by asserting such hops fail to parse and fall back to the
    /// peer instead of being accepted.
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
