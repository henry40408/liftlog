package auth

import (
	"net/http"
	"net/netip"
	"slices"
	"strings"

	"github.com/henry40408/liftlog/internal/config"
)

// ClientIP resolves the client IP for per-IP rate limiting.
//
// Only the header named by LIFTLOG_TRUSTED_PROXY_HEADER is ever read: presence
// proves nothing, since nginx forwards a client-forged X-Forwarded-For
// verbatim. TrustNoHeader means the TCP peer, always.
//
// The header is honoured only when the peer is loopback or listed in
// trustedProxies; an invalid peer (unknown) is untrusted.
//
// X-Forwarded-For: reads the last field line (appending proxies put the
// trusted hop there) and strictly its last hop; anything leftward is
// attacker-controlled, so an unparseable last hop falls back to the peer
// rather than scanning left. Assumes exactly one trusted proxy. Hops may carry
// a port (ip:port, [v6]:port). No CIDR support.
//
// All IPs are unmapped: a dual-stack listener sees IPv4 peers as
// ::ffff:a.b.c.d, which would otherwise fail the trust check and collapse every
// client into one bucket.
func ClientIP(peer netip.Addr, headers http.Header, header config.TrustedProxyHeader, trustedProxies []netip.Addr) netip.Addr {
	peer = peer.Unmap()
	fallback := peer
	if !peer.IsValid() {
		fallback = netip.AddrFrom4([4]byte{127, 0, 0, 1})
	}
	if !isTrustedPeer(peer, trustedProxies) {
		return fallback
	}

	var resolved netip.Addr
	switch header {
	case config.TrustXForwardedFor:
		resolved = forwardedForLastHop(headers)
	case config.TrustXRealIP:
		resolved = lastLineHop(headers, "X-Real-Ip")
	}
	if !resolved.IsValid() {
		return fallback
	}
	return resolved
}

func isTrustedPeer(peer netip.Addr, trustedProxies []netip.Addr) bool {
	if !peer.IsValid() {
		return false
	}
	if peer.IsLoopback() {
		return true
	}
	return slices.ContainsFunc(trustedProxies, func(t netip.Addr) bool { return t.Unmap() == peer })
}

// parseHop accepts ip, ip:port, [v6]:port or [v6]; trailing garbage after "]"
// is rejected, not truncated. IPv6 zones are rejected.
func parseHop(hop string) netip.Addr {
	hop = strings.TrimSpace(hop)
	if ip, err := netip.ParseAddr(hop); err == nil {
		return withoutZone(ip)
	}
	if addrPort, err := netip.ParseAddrPort(hop); err == nil {
		return withoutZone(addrPort.Addr())
	}
	inner, ok := strings.CutPrefix(hop, "[")
	if !ok {
		return netip.Addr{}
	}
	inner, ok = strings.CutSuffix(inner, "]")
	if !ok {
		return netip.Addr{}
	}
	ip, err := netip.ParseAddr(inner)
	if err != nil {
		return netip.Addr{}
	}
	return withoutZone(ip)
}

func withoutZone(ip netip.Addr) netip.Addr {
	if ip.Zone() != "" {
		return netip.Addr{}
	}
	return ip.Unmap()
}

func forwardedForLastHop(headers http.Header) netip.Addr {
	line, ok := lastLine(headers, "X-Forwarded-For")
	if !ok {
		return netip.Addr{}
	}
	return parseHop(line[strings.LastIndex(line, ",")+1:])
}

func lastLineHop(headers http.Header, name string) netip.Addr {
	line, ok := lastLine(headers, name)
	if !ok {
		return netip.Addr{}
	}
	return parseHop(line)
}

func lastLine(headers http.Header, name string) (string, bool) {
	values := headers.Values(name)
	if len(values) == 0 {
		return "", false
	}
	return values[len(values)-1], true
}
