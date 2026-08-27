use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn create_pool(database_url: &str) -> Result<DbPool, r2d2::Error> {
    let path = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
    let path = path.split('?').next().unwrap_or(path);

    if path == ":memory:" {
        // PRAGMA foreign_keys is per-connection. The bundled SQLite in this
        // build happens to compile with SQLITE_DEFAULT_FOREIGN_KEYS, so it
        // already defaults to ON here — but that default is a build-time
        // compile flag, not something this crate controls or can rely on
        // staying true (a non-bundled libsqlite3, or a different bundled
        // build, may default it to OFF). Migration 010 also demonstrated the
        // failure mode directly: it turns the pragma off for its own
        // rebuild and, by design, leaves the one pooled connection that ran
        // migrations with it off afterwards (see run_migrations' restore at
        // the end of the loop). Setting it explicitly here, in every pool's
        // connection initialiser, makes enforcement an invariant of this
        // codebase rather than an accident of how SQLite was compiled. All
        // three pool-construction paths in this file must agree on this
        // pragma, or enforcement would depend on which pooled connection a
        // given request happens to get.
        let manager = SqliteConnectionManager::memory()
            .with_init(|conn| conn.execute_batch("PRAGMA foreign_keys=ON;"));
        return Pool::builder().max_size(1).build(manager);
    }

    // WAL gives concurrent readers; busy_timeout absorbs lock contention from
    // the spawn_blocking pool (which runs many short writes via r2d2).
    let manager = SqliteConnectionManager::file(Path::new(path)).with_init(|conn| {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;\
             PRAGMA synchronous=NORMAL;\
             PRAGMA busy_timeout=5000;\
             PRAGMA foreign_keys=ON;",
        )
    });
    Pool::builder().max_size(10).build(manager)
}

/// Flush the WAL back into the main database and truncate the `-wal` file.
///
/// Run on graceful shutdown so the on-disk DB is self-contained; the `-wal`
/// and `-shm` siblings are then removed by `SQLite` when the pool's last
/// connection closes.
pub fn checkpoint(pool: &DbPool) -> anyhow::Result<()> {
    let conn = pool.get()?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

#[allow(dead_code)]
pub fn create_memory_pool() -> Result<DbPool, r2d2::Error> {
    // Must match create_pool's enforcement (see the comment there): this is
    // the pool tests/common/mod.rs::setup_test_db and the repository unit
    // tests build on, so if it disagreed with production, none of the
    // cascade/restrict behaviour changes would be covered by any test.
    let manager = SqliteConnectionManager::memory()
        .with_init(|conn| conn.execute_batch("PRAGMA foreign_keys=ON;"));
    Pool::builder().max_size(1).build(manager)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// File-pool test holder that deletes the DB file (and WAL/SHM siblings) on drop.
    struct TempDbPath(std::path::PathBuf);

    impl TempDbPath {
        fn new() -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("liftlog-test-{}.sqlite3", uuid::Uuid::new_v4()));
            Self(path)
        }

        fn url(&self) -> String {
            format!("sqlite:{}", self.0.display())
        }
    }

    impl Drop for TempDbPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
            let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
        }
    }

    #[test]
    fn create_pool_with_memory_url() {
        let pool = create_pool("sqlite::memory:").expect("memory pool");
        let conn = pool.get().expect("get conn");
        let one: i64 = conn
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("query");
        assert_eq!(one, 1);
    }

    #[test]
    fn create_pool_with_file_url_enables_wal_pragmas() {
        let tmp = TempDbPath::new();
        let pool = create_pool(&tmp.url()).expect("file pool");

        let conn = pool.get().expect("get conn");
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal_mode");
        assert_eq!(mode.to_lowercase(), "wal");

        let sync: i64 = conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .expect("synchronous");
        // NORMAL == 1 (FULL == 2, OFF == 0).
        assert_eq!(sync, 1);
    }

    #[test]
    fn checkpoint_truncates_wal_file() {
        let tmp = TempDbPath::new();
        let pool = create_pool(&tmp.url()).expect("file pool");
        {
            let conn = pool.get().expect("get conn");
            conn.execute_batch(
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);\
                 INSERT INTO t (v) VALUES ('a'), ('b'), ('c');",
            )
            .expect("write");
        }

        let wal_path = format!("{}-wal", tmp.0.display());
        let before = std::fs::metadata(&wal_path).expect("wal exists").len();
        assert!(before > 0, "WAL should have frames before checkpoint");

        checkpoint(&pool).expect("checkpoint");

        let after = std::fs::metadata(&wal_path)
            .expect("wal still exists")
            .len();
        assert_eq!(after, 0, "TRUNCATE checkpoint should zero the WAL file");
    }

    #[test]
    fn create_pool_strips_query_params_and_sqlite_prefix() {
        let pool = create_pool("sqlite::memory:?mode=rwc").expect("pool");
        assert!(pool.get().is_ok());
    }

    #[test]
    fn create_pool_enables_foreign_keys() {
        let tmp = TempDbPath::new();
        let pool = create_pool(&tmp.url()).expect("file pool");

        let conn = pool.get().expect("get conn");
        let enabled: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign_keys");
        assert_eq!(enabled, 1);
    }

    #[test]
    fn create_pool_memory_url_enables_foreign_keys() {
        let pool = create_pool("sqlite::memory:").expect("memory pool");

        let conn = pool.get().expect("get conn");
        let enabled: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign_keys");
        assert_eq!(enabled, 1);
    }

    #[test]
    fn memory_pool_enables_foreign_keys() {
        let pool = create_memory_pool().expect("memory pool");

        let conn = pool.get().expect("get conn");
        let enabled: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign_keys");
        assert_eq!(enabled, 1);
    }

    #[test]
    fn foreign_keys_enabled_on_every_pooled_connection() {
        let tmp = TempDbPath::new();
        let pool = create_pool(&tmp.url()).expect("file pool");

        // Hold several connections at once so r2d2 is forced to actually
        // create more than one, pinning that the pragma is set uniformly by
        // the pool's connection initialiser rather than by chance on
        // whichever single connection a test happens to grab.
        let conns: Vec<_> = (0..5).map(|_| pool.get().expect("get conn")).collect();

        for conn in &conns {
            let enabled: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .expect("foreign_keys");
            assert_eq!(enabled, 1);
        }
    }
}
