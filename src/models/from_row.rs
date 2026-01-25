use rusqlite::Row;

pub trait FromSqliteRow: Sized {
    fn from_row(row: &Row) -> rusqlite::Result<Self>;
}
