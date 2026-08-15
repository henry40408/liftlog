//! The LiftLog server under test.
//!
//! Replaces the `workerServer` fixture in `tests/e2e/steps/fixtures.js`: it
//! builds the debug binary if it is missing, starts it against a throwaway
//! SQLite file, waits for `/health`, and kills it on drop.
//!
//! Two things differ from the Playwright harness on purpose:
//!
//! * **One server, not one per worker.** Playwright ran a process per worker
//!   because each worker was an OS process with no way to share one. Cucumber
//!   runs its scenarios as tasks on a single runtime, so a single server serves
//!   all of them and the scenarios stay isolated the way they always did — by
//!   scoping their fixtures to a per-scenario suffix.
//! * **The database is never reused.** It is deleted before the server starts,
//!   which is what lets the `@bootstrap` scenarios assert on an install that
//!   has no users yet. There is deliberately no "adopt a server that is already
//!   listening" path: an adopted server would come with whatever users the last
//!   run left behind.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// The password every seeded account gets.
///
/// It goes through the real `/auth/setup` and `/users/new` endpoints, so it has
/// to satisfy the server's policy — a length floor plus a zxcvbn score of at
/// least 3 (`models::user::password_policy_error`). Comfortably above that bar
/// rather than exactly on it: a value scoring exactly 3 would fail the whole
/// suite at the seeding step if the threshold were ever raised, which is where
/// the cause is least visible.
pub const PASSWORD: &str = "barbell-club-2026";

/// The first account, created by `/auth/setup` and therefore an admin. Every
/// scenario that needs admin rights signs in as this one.
pub const ADMIN: &str = "lifter";

/// How long to wait for the server to answer `/health`.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// How often to re-check while waiting for it.
const STARTUP_INTERVAL: Duration = Duration::from_millis(100);

/// Where every page object and the seeding client point.
///
/// A `OnceLock` rather than a constant because the port is picked by the OS at
/// startup — a fixed port would collide with a second checkout running its own
/// suite, and this one cannot fall back to adopting that server.
static BASE_URL: OnceLock<String> = OnceLock::new();

/// The base URL of the server under test.
///
/// # Panics
///
/// Panics when no [`Server`] has been started — a step that ran without the
/// runner's setup.
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
    /// Fails when the binary cannot be built or spawned, when the database file
    /// cannot be removed, or when `/health` does not answer within
    /// [`STARTUP_TIMEOUT`].
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
            // Inherited, so a refusal to start is visible in the test output
            // rather than swallowed into a pipe nobody reads.
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawning the liftlog server at {}", binary.display()))?;

        // Bound before the wait, so a server that never answers is still killed
        // when the error propagates.
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

/// Path to the server binary, building it first when it is not there.
///
/// The **dev** profile, deliberately. The release profile is tuned for the
/// Docker image — `lto = true`, `codegen-units = 1`, `strip = true` — none of
/// which makes a local SQLite file answer faster, and it shares no artefacts
/// with `cargo nextest run`. CI builds it in an earlier step, so this is the
/// local-developer path.
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

/// The database path, with any file from a previous run removed.
///
/// The `-wal` and `-shm` sidecars go too: SQLite would replay a leftover WAL
/// against a database that no longer exists, which resurrects the users the
/// `@bootstrap` scenarios need gone.
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

/// A port nothing is listening on, by asking the OS for one and letting it go.
///
/// Racy in principle — another process could claim it in the gap — and settled
/// in practice by the fact that the only thing racing for ports here is another
/// checkout doing exactly this, which the OS hands a different number.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("asking the OS for a free port")?;
    Ok(listener.local_addr()?.port())
}

/// Waits for `/health` to answer, not merely for the port to open.
///
/// The listener is bound before the migrations finish, so a scenario that only
/// waited for the socket could hit a half-migrated database.
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

/// The repository root — the parent of this crate's directory.
fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("e2e/ always has a parent")
}
