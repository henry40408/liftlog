use std::env::{self, VarError};
use std::net::{IpAddr, SocketAddr};

/// Which proxy-supplied header, if any, may be trusted to carry the real
/// client IP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrustedProxyHeader {
    /// Trust no forwarding header; always use the TCP peer address.
    #[default]
    None,
    XForwardedFor,
    XRealIp,
}

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    pub trusted_proxy_header: TrustedProxyHeader,
    pub trusted_proxies: Vec<IpAddr>,
    pub cookie_secure: bool,
    pub hsts_max_age: u64,
    pub hsts_include_subdomains: bool,
}

/// Env vars that were renamed under the `LIFTLOG_` prefix, paired with their
/// current name. Old deployments that upgrade without renaming would otherwise
/// have their value silently ignored; [`reject_legacy_env_vars`] fails startup
/// so the misconfiguration is visible.
const RENAMED_ENV_VARS: &[(&str, &str)] = &[
    ("BIND", "LIFTLOG_BIND"),
    ("LOG_FORMAT", "LIFTLOG_LOG_FORMAT"),
];

/// Refuse to start when any pre-prefix env var name is still present, pointing
/// the operator at its replacement.
pub fn reject_legacy_env_vars() -> anyhow::Result<()> {
    let stale: Vec<String> = RENAMED_ENV_VARS
        .iter()
        .filter(|(old, _)| env::var_os(old).is_some())
        .map(|(old, new)| format!("{old} (renamed to {new})"))
        .collect();
    if !stale.is_empty() {
        anyhow::bail!(
            "refusing to start: removed environment variable(s) still set: {}. \
             Rename them in your deployment to the LIFTLOG_-prefixed names.",
            stale.join(", ")
        );
    }
    Ok(())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        reject_legacy_env_vars()?;
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:liftlog.sqlite3?mode=rwc".to_string()),
            bind: parse_bind(env::var("LIFTLOG_BIND").ok().as_deref())
                .map_err(anyhow::Error::msg)?,
            trusted_proxy_header: parse_trusted_proxy_header(
                read_env_var("LIFTLOG_TRUSTED_PROXY_HEADER")?.as_deref(),
            )
            .map_err(anyhow::Error::msg)?,
            trusted_proxies: parse_trusted_proxies(
                read_env_var("LIFTLOG_TRUSTED_PROXIES")?.as_deref(),
            )
            .map_err(anyhow::Error::msg)?,
            cookie_secure: parse_bool_env(
                "LIFTLOG_COOKIE_SECURE",
                read_env_var("LIFTLOG_COOKIE_SECURE")?.as_deref(),
                false,
            )
            .map_err(anyhow::Error::msg)?,
            hsts_max_age: parse_hsts_max_age(read_env_var("LIFTLOG_HSTS_MAX_AGE")?.as_deref())
                .map_err(anyhow::Error::msg)?,
            hsts_include_subdomains: parse_bool_env(
                "LIFTLOG_HSTS_INCLUDE_SUBDOMAINS",
                read_env_var("LIFTLOG_HSTS_INCLUDE_SUBDOMAINS")?.as_deref(),
                false,
            )
            .map_err(anyhow::Error::msg)?,
        })
    }
}

/// Reads an environment variable, distinguishing "unset" from "set but not
/// valid UTF-8" instead of collapsing both into `None` the way
/// `env::var(name).ok()` does. Collapsing them is dangerous for a strict
/// boolean flag like `LIFTLOG_COOKIE_SECURE`: `env::var` returns
/// `Err(VarError::NotUnicode(..))` for a non-UTF-8 value, and `.ok()` maps
/// that to `None`, which every caller here treats as "unset" and falls back
/// to its default. For `LIFTLOG_COOKIE_SECURE` the default is `false`, so a
/// non-UTF-8 value (a stray control character from a misconfigured secrets
/// manager, say) would silently deploy without the `Secure` cookie
/// attribute — exactly the misconfiguration `parse_bool_env`'s strictness
/// exists to catch. This surfaces `NotUnicode` as a hard error instead.
///
/// `LIFTLOG_BIND` and `DATABASE_URL` intentionally still use `env::var(..).ok()`
/// directly and are not routed through this helper: that is pre-existing
/// behaviour and out of scope for this change.
fn read_env_var(name: &str) -> anyhow::Result<Option<String>> {
    match env::var(name) {
        Ok(v) => Ok(Some(v)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => {
            anyhow::bail!("environment variable {name} is not valid UTF-8")
        }
    }
}

/// Resolve the `LIFTLOG_BIND` value into a [`SocketAddr`]. An unset or empty value
/// yields the default `127.0.0.1:8080` (loopback only, so a bare-metal run is
/// not exposed on all interfaces without opting in); any non-empty value must
/// be a valid `host:port` socket address. The container image sets
/// `LIFTLOG_BIND=0.0.0.0:8080` so a reverse proxy in a separate container can reach it.
pub fn parse_bind(raw: Option<&str>) -> Result<SocketAddr, String> {
    match raw {
        Some(v) if !v.is_empty() => v
            .parse::<SocketAddr>()
            .map_err(|e| format!("invalid LIFTLOG_BIND '{v}': {e}")),
        _ => Ok(SocketAddr::from(([127, 0, 0, 1], 8080))),
    }
}

/// Resolve the `LIFTLOG_TRUSTED_PROXIES` value into a list of bare IPs whose
/// forwarding header may be trusted for client-IP resolution (see
/// [`crate::net::client_ip`]). Unset, empty, or whitespace-only input yields
/// an empty `Vec` (no proxy trusted beyond loopback). Otherwise the value is
/// a comma-separated list; each segment is trimmed and empty segments are
/// skipped, so a trailing comma is tolerated. Any non-empty segment that
/// does not parse as a bare IP is a hard error. No CIDR support: that would
/// need a new dependency.
///
/// Each parsed IP is passed through [`IpAddr::to_canonical`] so
/// `Config::trusted_proxies` is canonical by construction — e.g.
/// `::ffff:10.0.0.5` becomes `10.0.0.5`, matching a plain-IPv4 peer address.
/// Without this, comparing against a peer canonicalized by
/// [`crate::net::client_ip`] would silently fail for entries written in
/// IPv4-mapped-IPv6 form.
pub fn parse_trusted_proxies(raw: Option<&str>) -> Result<Vec<IpAddr>, String> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    raw.split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            segment
                .parse::<IpAddr>()
                .map(|ip| ip.to_canonical())
                .map_err(|e| format!("invalid LIFTLOG_TRUSTED_PROXIES entry '{segment}': {e}"))
        })
        .collect()
}

/// Resolve the `LIFTLOG_TRUSTED_PROXY_HEADER` value into a [`TrustedProxyHeader`].
/// Unset, empty, or whitespace-only input yields [`TrustedProxyHeader::None`]
/// — liftlog cannot tell whether a forwarding header was written by a
/// trusted proxy or passed through verbatim from the client, so honouring
/// one has to be an explicit operator statement that their proxy overwrites
/// (or strips) that header. Defaulting to trusting `X-Forwarded-For` would
/// make the login rate limit bypassable on the extremely common minimal
/// nginx config that only sets `X-Real-IP` and forwards a client-supplied
/// `X-Forwarded-For` through untouched. With `None`, clients behind a proxy
/// all share the proxy's single rate-limit bucket — a real limitation, but a
/// safe one, and still strictly better than no throttle at all.
///
/// Accepts `none`, `x-forwarded-for`, `x-real-ip`, case-insensitively and
/// trimmed. Any other non-empty value is a hard error.
pub fn parse_trusted_proxy_header(raw: Option<&str>) -> Result<TrustedProxyHeader, String> {
    let Some(raw) = raw else {
        return Ok(TrustedProxyHeader::None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(TrustedProxyHeader::None);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "none" => Ok(TrustedProxyHeader::None),
        "x-forwarded-for" => Ok(TrustedProxyHeader::XForwardedFor),
        "x-real-ip" => Ok(TrustedProxyHeader::XRealIp),
        _ => Err(format!(
            "invalid LIFTLOG_TRUSTED_PROXY_HEADER '{raw}': expected one of none, x-forwarded-for, x-real-ip"
        )),
    }
}

/// Strict boolean env-var parser. Accepts `true` / `false` / `1` / `0`,
/// case-insensitively, after trimming; unset or empty means unset.
/// An unrecognised value is a hard error rather than a silent `false`.
///
/// Strictness is the point: the default is `false`, so if a typo like
/// `LIFTLOG_COOKIE_SECURE=yes` were treated as "off", a correctly-configured HTTPS
/// deployment would silently lose `Secure` — exactly what this setting
/// exists to prevent.
pub fn parse_bool_env(name: &str, raw: Option<&str>, default: bool) -> Result<bool, String> {
    let Some(v) = raw else {
        return Ok(default);
    };
    let trimmed = v.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(format!(
            "invalid {name} '{v}': expected one of true, false, 1, 0"
        )),
    }
}

/// Resolve `LIFTLOG_HSTS_MAX_AGE` (seconds). Unset, empty, or `0` means no
/// `Strict-Transport-Security` header is sent.
///
/// Default-off is deliberate: liftlog never terminates TLS (see the README),
/// so it cannot tell whether a request actually arrived over HTTPS — it can
/// only trust that the operator's proxy is doing the right thing. HSTS is a
/// browser-enforced promise that this domain is HTTPS-only for `max_age`
/// seconds, and a wrong promise cannot be withdrawn from the server side —
/// there is no "un-send" a `Strict-Transport-Security` header already cached
/// by a browser; the only fix is waiting out the `max-age`. The layer that
/// actually terminates TLS (the reverse proxy) is the layer that can vouch
/// for HTTPS and the layer operators should prefer for sending this header;
/// see the README for the fuller rationale.
pub fn parse_hsts_max_age(raw: Option<&str>) -> Result<u64, String> {
    let Some(raw) = raw else {
        return Ok(0);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    trimmed
        .parse::<u64>()
        .map_err(|e| format!("invalid LIFTLOG_HSTS_MAX_AGE '{raw}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn parse_bind_defaults_when_absent_or_empty() {
        assert_eq!(
            parse_bind(None).unwrap(),
            SocketAddr::from(([127, 0, 0, 1], 8080))
        );
        assert_eq!(
            parse_bind(Some("")).unwrap(),
            SocketAddr::from(([127, 0, 0, 1], 8080))
        );
    }

    #[test]
    fn parse_bind_accepts_valid_socket_addr() {
        assert_eq!(
            parse_bind(Some("127.0.0.1:9000")).unwrap(),
            "127.0.0.1:9000".parse().unwrap()
        );
        assert_eq!(
            parse_bind(Some("0.0.0.0:8080")).unwrap(),
            "0.0.0.0:8080".parse().unwrap()
        );
    }

    #[test]
    fn parse_bind_rejects_invalid() {
        // Invalid input fails with a descriptive error; a bare host with no
        // port is not a SocketAddr.
        let err = parse_bind(Some("not-an-addr")).unwrap_err();
        assert!(err.contains("invalid LIFTLOG_BIND"), "got: {err}");
        assert!(parse_bind(Some("127.0.0.1")).is_err());
    }

    #[test]
    fn from_env_reads_bind() {
        // nextest runs each test in its own process, so mutating the
        // environment here does not leak into other tests. `set_var` is
        // `unsafe` under edition 2024.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("LIFTLOG_BIND", "127.0.0.1:9137");
        }
        let config = Config::from_env().expect("from_env should succeed");
        assert_eq!(config.bind, "127.0.0.1:9137".parse().unwrap());
    }

    #[test]
    fn from_env_rejects_legacy_bind() {
        // A pre-prefix name still set means the deployment wasn't migrated;
        // startup must fail with a message naming both the old and new var.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("BIND", "0.0.0.0:8080");
        }
        assert!(
            Config::from_env().is_err(),
            "legacy BIND should fail startup"
        );
        let msg = reject_legacy_env_vars().unwrap_err().to_string();
        assert!(msg.contains("BIND"), "got: {msg}");
        assert!(msg.contains("LIFTLOG_BIND"), "got: {msg}");
    }

    #[test]
    fn from_env_rejects_legacy_log_format() {
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("LOG_FORMAT", "json");
        }
        assert!(
            Config::from_env().is_err(),
            "legacy LOG_FORMAT should fail startup"
        );
        let msg = reject_legacy_env_vars().unwrap_err().to_string();
        assert!(msg.contains("LOG_FORMAT"), "got: {msg}");
        assert!(msg.contains("LIFTLOG_LOG_FORMAT"), "got: {msg}");
    }

    #[test]
    fn reject_legacy_env_vars_passes_when_clean() {
        // A process with no legacy names set (nextest isolates each test) is
        // accepted.
        reject_legacy_env_vars().expect("no legacy vars should pass");
    }

    #[test]
    fn parse_trusted_proxies_defaults_empty() {
        assert_eq!(parse_trusted_proxies(None).unwrap(), Vec::<IpAddr>::new());
        assert_eq!(
            parse_trusted_proxies(Some("")).unwrap(),
            Vec::<IpAddr>::new()
        );
        assert_eq!(
            parse_trusted_proxies(Some("   ")).unwrap(),
            Vec::<IpAddr>::new()
        );
    }

    #[test]
    fn parse_trusted_proxies_accepts_comma_separated_ips() {
        let parsed = parse_trusted_proxies(Some(" 10.0.0.1 , ::1, 192.168.1.1 , ")).unwrap();
        assert_eq!(
            parsed,
            vec![
                "10.0.0.1".parse::<IpAddr>().unwrap(),
                "::1".parse::<IpAddr>().unwrap(),
                "192.168.1.1".parse::<IpAddr>().unwrap(),
            ]
        );
    }

    #[test]
    fn parse_trusted_proxies_rejects_garbage() {
        let err = parse_trusted_proxies(Some("10.0.0.1, not-an-ip")).unwrap_err();
        assert!(
            err.contains("invalid LIFTLOG_TRUSTED_PROXIES"),
            "got: {err}"
        );
    }

    #[test]
    fn parse_trusted_proxies_canonicalizes_mapped_ipv6() {
        let parsed = parse_trusted_proxies(Some("::ffff:10.0.0.5")).unwrap();
        assert_eq!(parsed, vec!["10.0.0.5".parse::<IpAddr>().unwrap()]);
    }

    #[test]
    fn parse_trusted_proxy_header_defaults_to_none() {
        assert_eq!(
            parse_trusted_proxy_header(None).unwrap(),
            TrustedProxyHeader::None
        );
        assert_eq!(
            parse_trusted_proxy_header(Some("")).unwrap(),
            TrustedProxyHeader::None
        );
        assert_eq!(
            parse_trusted_proxy_header(Some("   ")).unwrap(),
            TrustedProxyHeader::None
        );
    }

    #[test]
    fn parse_trusted_proxy_header_accepts_known_values_case_insensitively() {
        assert_eq!(
            parse_trusted_proxy_header(Some("X-Forwarded-For")).unwrap(),
            TrustedProxyHeader::XForwardedFor
        );
        assert_eq!(
            parse_trusted_proxy_header(Some("x-real-ip")).unwrap(),
            TrustedProxyHeader::XRealIp
        );
        assert_eq!(
            parse_trusted_proxy_header(Some("NONE")).unwrap(),
            TrustedProxyHeader::None
        );
        assert_eq!(
            parse_trusted_proxy_header(Some("  x-forwarded-for  ")).unwrap(),
            TrustedProxyHeader::XForwardedFor
        );
    }

    #[test]
    fn parse_trusted_proxy_header_rejects_unknown() {
        let err = parse_trusted_proxy_header(Some("x-forwarded")).unwrap_err();
        assert!(
            err.contains("invalid LIFTLOG_TRUSTED_PROXY_HEADER"),
            "got: {err}"
        );
    }

    #[test]
    fn parse_bool_env_defaults_when_absent_or_empty() {
        assert!(!parse_bool_env("LIFTLOG_COOKIE_SECURE", None, false).unwrap());
        assert!(parse_bool_env("LIFTLOG_COOKIE_SECURE", None, true).unwrap());
        assert!(!parse_bool_env("LIFTLOG_COOKIE_SECURE", Some(""), false).unwrap());
        assert!(parse_bool_env("LIFTLOG_COOKIE_SECURE", Some(""), true).unwrap());
        assert!(!parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("   "), false).unwrap());
        assert!(parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("   "), true).unwrap());
    }

    #[test]
    fn parse_bool_env_accepts_true_false_1_0_case_insensitively() {
        assert!(parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("true"), false).unwrap());
        assert!(parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("TRUE"), false).unwrap());
        assert!(parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("  true  "), false).unwrap());
        assert!(parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("1"), false).unwrap());

        assert!(!parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("false"), true).unwrap());
        assert!(!parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("False"), true).unwrap());
        assert!(!parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("  false  "), true).unwrap());
        assert!(!parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("0"), true).unwrap());
    }

    #[test]
    fn parse_bool_env_rejects_unrecognised_value() {
        let err = parse_bool_env("LIFTLOG_COOKIE_SECURE", Some("yes"), false).unwrap_err();
        assert!(err.contains("invalid LIFTLOG_COOKIE_SECURE"), "got: {err}");
    }

    #[test]
    fn read_env_var_returns_none_when_unset() {
        // nextest runs each test in its own process, so this name is safe to
        // assume absent.
        assert!(
            read_env_var("LIFTLOG_TEST_DEFINITELY_UNSET_VAR")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn read_env_var_returns_value_when_present() {
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("LIFTLOG_TEST_READ_ENV_VAR_PRESENT", "hello");
        }
        assert_eq!(
            read_env_var("LIFTLOG_TEST_READ_ENV_VAR_PRESENT").unwrap(),
            Some("hello".to_string())
        );
    }

    // Only unix lets a test construct a non-UTF-8 `OsString` to exercise the
    // `VarError::NotUnicode` branch; there is no portable, safe way to do
    // this, so the test is unix-only rather than skipped entirely.
    #[cfg(unix)]
    #[test]
    fn read_env_var_errors_on_non_utf8() {
        use std::os::unix::ffi::OsStringExt;

        let name = "LIFTLOG_TEST_NON_UTF8_CONFIG_VAR";
        let invalid = std::ffi::OsString::from_vec(vec![0x66, 0x6f, 0x80, 0x6f]);
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var(name, &invalid);
        }

        let err = read_env_var(name).unwrap_err();
        assert!(err.to_string().contains(name), "got: {err}");

        #[allow(unsafe_code)]
        unsafe {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn parse_hsts_max_age_defaults_to_zero() {
        assert_eq!(parse_hsts_max_age(None).unwrap(), 0);
        assert_eq!(parse_hsts_max_age(Some("")).unwrap(), 0);
        assert_eq!(parse_hsts_max_age(Some("   ")).unwrap(), 0);
    }

    #[test]
    fn parse_hsts_max_age_accepts_seconds() {
        assert_eq!(parse_hsts_max_age(Some("31536000")).unwrap(), 31_536_000);
        assert_eq!(parse_hsts_max_age(Some(" 0 ")).unwrap(), 0);
    }

    #[test]
    fn parse_hsts_max_age_rejects_garbage() {
        let err = parse_hsts_max_age(Some("not-a-number")).unwrap_err();
        assert!(err.contains("invalid LIFTLOG_HSTS_MAX_AGE"), "got: {err}");

        // Must not silently wrap to a huge u64 — a negative value is a hard
        // error, not "unsigned integer parsing accepts it somehow".
        let err = parse_hsts_max_age(Some("-1")).unwrap_err();
        assert!(err.contains("invalid LIFTLOG_HSTS_MAX_AGE"), "got: {err}");
    }
}
