-- Rebuild sessions table to add last_touched_at NOT NULL.
-- SQLite does not permit ALTER TABLE ADD COLUMN NOT NULL DEFAULT CURRENT_TIMESTAMP,
-- so we use the canonical 12-step rebuild. No tables reference sessions, so
-- turning foreign keys off is defensive only.
PRAGMA foreign_keys = OFF;

CREATE TABLE sessions_new (
    token TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    last_touched_at DATETIME NOT NULL
);

INSERT INTO sessions_new (token, user_id, created_at, expires_at, last_touched_at)
    SELECT token, user_id, created_at, expires_at, created_at FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_new RENAME TO sessions;

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

PRAGMA foreign_keys = ON;
