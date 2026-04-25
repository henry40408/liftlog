use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;

pub type DbPool = Pool<SqliteConnectionManager>;
#[allow(dead_code)]
pub type DbConnection = PooledConnection<SqliteConnectionManager>;

pub fn create_pool(database_url: &str) -> Result<DbPool, r2d2::Error> {
    let path = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
    // Remove query parameters (e.g., ?mode=rwc)
    let path = path.split('?').next().unwrap_or(path);

    if path == ":memory:" {
        return Pool::builder()
            .max_size(1)
            .build(SqliteConnectionManager::memory());
    }

    // WAL gives concurrent readers; busy_timeout absorbs lock contention from
    // the spawn_blocking pool (which runs many short writes via r2d2).
    let manager = SqliteConnectionManager::file(Path::new(path)).with_init(|conn| {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;\
             PRAGMA synchronous=NORMAL;\
             PRAGMA busy_timeout=5000;",
        )
    });
    Pool::builder().max_size(10).build(manager)
}

#[allow(dead_code)]
pub fn create_memory_pool() -> Result<DbPool, r2d2::Error> {
    let manager = SqliteConnectionManager::memory();
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
    fn create_pool_strips_query_params_and_sqlite_prefix() {
        let pool = create_pool("sqlite::memory:?mode=rwc").expect("pool");
        assert!(pool.get().is_ok());
    }
}
