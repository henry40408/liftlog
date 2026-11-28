package migrations

// release031Schema is sqlite_master of a database that 0.4 or later created and
// releases up to 0.31 brought to their last migration, as SQLite stored the
// statements. Databases created before 0.4 also keep columns and a table that are
// not here. It is never regenerated from the baseline.
const release031Schema = `
CREATE TABLE _migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
, role TEXT NOT NULL DEFAULT 'user', timezone TEXT);
CREATE INDEX idx_users_username ON users(username);
CREATE TABLE exercises (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    user_id TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX idx_exercises_category ON exercises(category);
CREATE INDEX idx_exercises_user_id ON exercises(user_id);
CREATE TABLE workout_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    date DATE NOT NULL,
    notes TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP, share_token TEXT, share_expires_at DATETIME,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX idx_workout_sessions_user_id ON workout_sessions(user_id);
CREATE INDEX idx_workout_sessions_date ON workout_sessions(date);
CREATE TABLE workout_logs (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    set_number INTEGER NOT NULL,
    reps INTEGER NOT NULL,
    weight REAL NOT NULL,
    rpe INTEGER,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (session_id) REFERENCES workout_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (exercise_id) REFERENCES exercises(id) ON DELETE RESTRICT
);
CREATE INDEX idx_workout_logs_session_id ON workout_logs(session_id);
CREATE INDEX idx_workout_logs_exercise_id ON workout_logs(exercise_id);
CREATE INDEX idx_workout_logs_exercise_weight ON workout_logs(exercise_id, weight DESC);
CREATE UNIQUE INDEX idx_workout_sessions_share_token
    ON workout_sessions(share_token) WHERE share_token IS NOT NULL;
CREATE TABLE "sessions" (
    token TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    last_touched_at DATETIME NOT NULL
);
CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);
`

// release031Rows are the records those releases wrote and a row in every table,
// plus two records only releases up to 0.3 wrote, to show unknown ones are kept.
const release031Rows = `
INSERT INTO _migrations (name, applied_at) VALUES
    ('001_create_users.sql', '2026-02-01 00:00:00'),
    ('002_create_exercises.sql', '2026-02-01 00:00:00'),
    ('003_create_workout_sessions.sql', '2026-02-01 00:00:00'),
    ('004_create_workout_logs.sql', '2026-02-01 00:00:00'),
    ('005_create_personal_records.sql', '2026-02-01 00:00:00'),
    ('006_seed_default_exercises.sql', '2026-02-01 00:00:00'),
    ('007_add_user_role.sql', '2026-02-01 00:00:00'),
    ('008_create_sessions.sql', '2026-03-01 00:00:00'),
    ('009_add_workout_share_token.sql', '2026-04-01 00:00:00'),
    ('010_rebuild_sessions_with_last_touched_at.sql', '2026-05-01 00:00:00'),
    ('011_cleanup_orphan_rows.sql', '2026-08-01 00:00:00'),
    ('012_add_workout_share_expires_at.sql', '2026-08-01 00:00:00'),
    ('013_add_user_timezone.sql', '2026-11-02 00:00:00');
INSERT INTO users VALUES ('u1', 'root', 'hash', '2026-02-01 00:00:00', 'admin', 'Asia/Taipei');
INSERT INTO exercises VALUES ('e1', 'Squat', 'legs', 'u1');
INSERT INTO workout_sessions VALUES ('w1', 'u1', '2026-11-10', 'notes', '2026-11-10 00:00:00', 'share', NULL);
INSERT INTO workout_logs VALUES ('l1', 'w1', 'e1', 1, 5, 102.5, 8, '2026-11-10 00:00:00');
INSERT INTO sessions VALUES ('t1', 'u1', '2026-11-10 00:00:00', '2026-11-17 00:00:00', '2026-11-10 00:00:00');
`
