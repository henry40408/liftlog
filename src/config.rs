use std::env;
use std::net::SocketAddr;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:liftlog.sqlite3?mode=rwc".to_string()),
            bind: parse_bind(env::var("BIND").ok().as_deref()).map_err(anyhow::Error::msg)?,
        })
    }
}

/// Resolve the `BIND` value into a [`SocketAddr`]. An unset or empty value
/// yields the default `127.0.0.1:8080` (loopback only, so a bare-metal run is
/// not exposed on all interfaces without opting in); any non-empty value must
/// be a valid `host:port` socket address. The container image sets
/// `BIND=0.0.0.0:8080` so a reverse proxy in a separate container can reach it.
pub fn parse_bind(raw: Option<&str>) -> Result<SocketAddr, String> {
    match raw {
        Some(v) if !v.is_empty() => v
            .parse::<SocketAddr>()
            .map_err(|e| format!("invalid BIND '{v}': {e}")),
        _ => Ok(SocketAddr::from(([127, 0, 0, 1], 8080))),
    }
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
        assert!(err.contains("invalid BIND"), "got: {err}");
        assert!(parse_bind(Some("127.0.0.1")).is_err());
    }

    #[test]
    fn from_env_reads_bind() {
        // nextest runs each test in its own process, so mutating the
        // environment here does not leak into other tests. `set_var` is
        // `unsafe` under edition 2024.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("BIND", "127.0.0.1:9137");
        }
        let config = Config::from_env().expect("from_env should succeed");
        assert_eq!(config.bind, "127.0.0.1:9137".parse().unwrap());
    }
}
