use std::env;
use std::net::{IpAddr, SocketAddr};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    pub trusted_proxies: Vec<IpAddr>,
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
            trusted_proxies: parse_trusted_proxies(
                env::var("LIFTLOG_TRUSTED_PROXIES").ok().as_deref(),
            )
            .map_err(anyhow::Error::msg)?,
        })
    }
}

/// Resolve the `LIFTLOG_BIND` value into a [`SocketAddr`]. An unset or empty
/// value yields the default `127.0.0.1:8080` (loopback only, so a bare-metal run
/// is not exposed on all interfaces without opting in); any non-empty value must
/// be a valid `host:port` socket address. The container image sets
/// `LIFTLOG_BIND=0.0.0.0:8080` so a reverse proxy in a separate container can
/// reach it.
pub fn parse_bind(raw: Option<&str>) -> Result<SocketAddr, String> {
    match raw {
        Some(v) if !v.is_empty() => v
            .parse::<SocketAddr>()
            .map_err(|e| format!("invalid LIFTLOG_BIND '{v}': {e}")),
        _ => Ok(SocketAddr::from(([127, 0, 0, 1], 8080))),
    }
}

/// Resolve the `LIFTLOG_TRUSTED_PROXIES` value into a list of bare IPs whose
/// `X-Forwarded-For` header may be trusted for client-IP resolution (see
/// [`crate::net::client_ip`]). Unset, empty, or whitespace-only input yields
/// an empty `Vec` (no proxy trusted beyond loopback). Otherwise the value is
/// a comma-separated list; each segment is trimmed and empty segments are
/// skipped, so a trailing comma is tolerated. Any non-empty segment that
/// does not parse as a bare IP is a hard error. No CIDR support: that would
/// need a new dependency.
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
                .map_err(|e| format!("invalid LIFTLOG_TRUSTED_PROXIES entry '{segment}': {e}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn parse_bind_defaults_when_absent_or_empty() {
        // Unset or empty → default 127.0.0.1:8080 (loopback only).
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
        // A valid host:port is honored, incl. a loopback-only bind.
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
        assert!(err.contains("invalid LIFTLOG_TRUSTED_PROXIES"), "got: {err}");
    }
}
