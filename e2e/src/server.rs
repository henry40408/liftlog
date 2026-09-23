//! The LiftLog server under test: built if missing, started on a free port
//! against a fresh SQLite file, killed on drop. One server serves every
//! scenario; the database is never reused, so `@bootstrap` sees no users.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// The password every seeded account gets. Must pass the server's policy
/// (`models::user::password_policy_error`), with margin so a stricter
/// threshold does not break seeding.
pub const PASSWORD: &str = "barbell-club-2026";

/// The first account, created by `/auth/setup` and therefore the admin.
pub const ADMIN: &str = "lifter";

const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const STARTUP_INTERVAL: Duration = Duration::from_millis(100);

/// Set at startup: the OS picks the port, so parallel checkouts don't collide.
static BASE_URL: OnceLock<String> = OnceLock::new();

/// The base URL of the server under test.
///
/// # Panics
///
/// Panics when no [`Server`] has been started.
pub fn base_url() -> &'static str {
    BASE_URL
        .get()
        .expect("no server started: `Server::start` runs before any scenario")
}

/// Joins a path onto [`base_url`].
pub fn url(path: &str) -> String {
    format!("{}{path}", base_url())
}

/// A running LiftLog server, killed when dropped.
pub struct Server {
    child: Child,
}

impl Server {
    /// Starts the server on a free port against a fresh database.
    ///
    /// # Errors
    ///
    /// Fails when the binary cannot be built or spawned, the database cannot be
    /// removed, or `/health` does not answer within [`STARTUP_TIMEOUT`].
    pub async fn start() -> Result<Self> {
        let binary = ensure_binary()?;
        let database = fresh_database()?;
        let port = free_port()?;

        let child = Command::new(&binary)
            .current_dir(repo_root())
            .env(
                "DATABASE_URL",
                format!("sqlite:{}?mode=rwc", database.display()),
            )
            .env("LIFTLOG_BIND", format!("127.0.0.1:{port}"))
            .env("RUST_LOG", "liftlog=warn")
            // Inherited so startup failures show in the test output.
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawning the liftlog server at {}", binary.display()))?;

        // Bound before the wait so a server that never answers is still killed.
        let server = Self { child };
        let base = format!("http://127.0.0.1:{port}");
        wait_until_healthy(&base).await?;
        BASE_URL
            .set(base)
            .map_err(|_| anyhow::anyhow!("a server was already started"))?;
        Ok(server)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Path to the dev-profile server binary, built only when absent — a stale
/// binary is used as-is, so run `cargo build` after changing the server.
fn ensure_binary() -> Result<PathBuf> {
    let binary = repo_root().join("target/debug/liftlog");
    if binary.is_file() {
        return Ok(binary);
    }

    eprintln!("e2e: {} is missing — building it", binary.display());
    let status = Command::new("cargo")
        .current_dir(repo_root())
        .arg("build")
        .status()
        .context("running `cargo build`")?;
    if !status.success() {
        bail!("`cargo build` failed with {status}");
    }
    if !binary.is_file() {
        bail!("`cargo build` did not produce {}", binary.display());
    }
    Ok(binary)
}

/// The database path, with any previous file removed — including the `-wal`
/// and `-shm` sidecars, whose replay would resurrect old users.
fn fresh_database() -> Result<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".tmp");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let database = dir.join("liftlog-e2e.sqlite3");
    for suffix in ["", "-wal", "-shm"] {
        let path = PathBuf::from(format!("{}{suffix}", database.display()));
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("removing {}", path.display())),
        }
    }
    Ok(database)
}

/// A free port, from the OS. Racy in principle, fine in practice.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("asking the OS for a free port")?;
    Ok(listener.local_addr()?.port())
}

/// Polls `/health` until the server answers.
async fn wait_until_healthy(base: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let health = format!("{base}/health");
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut last: Option<String> = None;

    while Instant::now() < deadline {
        match client.get(&health).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => last = Some(format!("{} from /health", response.status())),
            Err(err) => last = Some(err.to_string()),
        }
        tokio::time::sleep(STARTUP_INTERVAL).await;
    }

    bail!(
        "the liftlog server did not become healthy at {base} within {STARTUP_TIMEOUT:?}: {}",
        last.unwrap_or_else(|| "no response".to_string())
    )
}

/// The repository root.
fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("e2e/ always has a parent")
}
