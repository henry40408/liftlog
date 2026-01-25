use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;

pub type DbPool = Pool<SqliteConnectionManager>;
pub type DbConnection = PooledConnection<SqliteConnectionManager>;

pub fn create_pool(database_url: &str) -> Result<DbPool, r2d2::Error> {
    let path = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
    // Remove query parameters (e.g., ?mode=rwc)
    let path = path.split('?').next().unwrap_or(path);

    let manager = if path == ":memory:" {
        SqliteConnectionManager::memory()
    } else {
        SqliteConnectionManager::file(Path::new(path))
    };

    Pool::builder()
        .max_size(5)
        .build(manager)
}

pub fn create_memory_pool() -> Result<DbPool, r2d2::Error> {
    let manager = SqliteConnectionManager::memory();
    Pool::builder()
        .max_size(1)
        .build(manager)
}
