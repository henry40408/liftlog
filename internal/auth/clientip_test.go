package auth

import (
	"net/http"
	"net/netip"
	"testing"

	"github.com/henry40408/liftlog/internal/config"
)

type clientIPCase struct {
	name    string
	peer    string // empty means no peer
	lines   map[string][]string
	header  config.TrustedProxyHeader
	trusted []string
	want    string // empty means the peer
}

func addr(s string) netip.Addr {
	if s == "" {
		return netip.Addr{}
	}
	return netip.MustParseAddr(s)
}

func addrs(list []string) []netip.Addr {
	var out []netip.Addr
	for _, s := range list {
		out = append(out, addr(s))
	}
	return out
}

func xff(lines ...string) map[string][]string { return map[string][]string{"X-Forwarded-For": lines} }
func realIP(lines ...string) map[string][]string {
	return map[string][]string{"X-Real-Ip": lines}
}

func TestClientIP(t *testing.T) {
	const peer, proxy = "127.0.0.1", "198.51.100.7"
	cases := []clientIPCase{
		{name: "untrusted peer headers are ignored", peer: "203.0.113.9", lines: xff(proxy), header: config.TrustXForwardedFor},
		{name: "missing peer ignores headers", lines: xff(proxy), header: config.TrustXForwardedFor, want: "127.0.0.1"},
		{name: "loopback peer headers are honoured", peer: peer, lines: xff(proxy), header: config.TrustXForwardedFor, want: proxy},
		{name: "listed trusted proxy headers are honoured", peer: "10.0.0.5", lines: xff(proxy), header: config.TrustXForwardedFor, trusted: []string{"10.0.0.5"}, want: proxy},
		{name: "xff takes the rightmost hop", peer: peer, lines: xff("203.0.113.1, " + proxy), header: config.TrustXForwardedFor, want: proxy},
		{name: "x-real-ip is honoured when configured", peer: peer, lines: realIP("198.51.100.9"), header: config.TrustXRealIP, want: "198.51.100.9"},
		{name: "unparseable xff falls back to peer", peer: peer, lines: xff("not-an-ip"), header: config.TrustXForwardedFor},
		{name: "header none never reads any header", peer: peer, lines: map[string][]string{"X-Forwarded-For": {proxy}, "X-Real-Ip": {"198.51.100.9"}}, header: config.TrustNoHeader},
		{name: "x-real-ip ignored when configured header is xff", peer: peer, lines: realIP("198.51.100.9"), header: config.TrustXForwardedFor},
		{name: "xff ignored when configured header is x-real-ip", peer: peer, lines: xff(proxy), header: config.TrustXRealIP},
		{name: "duplicate xff lines use the last line", peer: peer, lines: xff("9.9.9.9", proxy), header: config.TrustXForwardedFor, want: proxy},
		{name: "duplicate x-real-ip lines use the last line", peer: peer, lines: realIP("9.9.9.9", proxy), header: config.TrustXRealIP, want: proxy},
		{name: "xff hop with port is parsed", peer: peer, lines: xff("9.9.9.9, 203.0.113.5:41234"), header: config.TrustXForwardedFor, want: "203.0.113.5"},
		{name: "bracketed ipv6 with port is parsed", peer: peer, lines: xff("[2001:db8::1]:443"), header: config.TrustXForwardedFor, want: "2001:db8::1"},
		{name: "bracketed ipv6 without port is parsed", peer: peer, lines: xff("[2001:db8::1]"), header: config.TrustXForwardedFor, want: "2001:db8::1"},
		{name: "trailing junk after bracket is rejected", peer: peer, lines: xff("[9.9.9.9]junk"), header: config.TrustXForwardedFor},
		{name: "double port is rejected", peer: peer, lines: xff("[::1]:80:80"), header: config.TrustXForwardedFor},
		{name: "empty port is rejected", peer: peer, lines: xff("[::1]:"), header: config.TrustXForwardedFor},
		{name: "ipv6 zone is rejected", peer: peer, lines: xff("fe80::1%eth0"), header: config.TrustXForwardedFor},
		{name: "mapped ipv6 loopback peer is trusted", peer: "::ffff:127.0.0.1", lines: xff(proxy), header: config.TrustXForwardedFor, want: proxy},
		{name: "mapped ipv6 peer matches a listed proxy", peer: "::ffff:10.0.0.5", lines: xff(proxy), header: config.TrustXForwardedFor, trusted: []string{"10.0.0.5"}, want: proxy},
		{name: "mapped ipv6 peer is canonicalised in the fallback", peer: "::ffff:203.0.113.9", header: config.TrustXForwardedFor, want: "203.0.113.9"},
		{name: "mapped ipv6 hop is canonicalised", peer: peer, lines: xff("::ffff:198.51.100.7"), header: config.TrustXForwardedFor, want: proxy},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			headers := http.Header{}
			for name, lines := range tc.lines {
				for _, line := range lines {
					headers.Add(name, line)
				}
			}
			want := addr(tc.want)
			if tc.want == "" {
				want = addr(tc.peer).Unmap()
			}
			got := ClientIP(addr(tc.peer), headers, tc.header, addrs(tc.trusted))
			if got != want {
				t.Fatalf("ClientIP = %v, want %v", got, want)
			}
		})
	}
}
