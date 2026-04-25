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
