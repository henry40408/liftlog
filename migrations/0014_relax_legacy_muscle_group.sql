-- Databases created before 0.4 require exercises.muscle_group and give it no
-- default, so adding an exercise fails. Rebuilding exercises would cascade into
-- the rows that reference it, so only the stored definition changes, as
-- https://sqlite.org/lang_altertable.html allows for removing NOT NULL. Where the
-- column does not exist, nothing matches.
PRAGMA writable_schema = ON;

UPDATE sqlite_master
SET sql = replace(sql, 'muscle_group TEXT NOT NULL', 'muscle_group TEXT')
WHERE type = 'table' AND name = 'exercises';

PRAGMA writable_schema = RESET;

-- The edit above does not tell other connections to read the schema again; any
-- schema statement does.
CREATE TABLE IF NOT EXISTS _schema_reload (unused INTEGER);
DROP TABLE IF EXISTS _schema_reload;
