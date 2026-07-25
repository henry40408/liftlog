-- Optional expiry for a workout's public share link. NULL means "never
-- expires", preserving the behaviour introduced with share_token in 009, so
-- existing share links keep working and no backfill is needed.
ALTER TABLE workout_sessions ADD COLUMN share_expires_at DATETIME;
