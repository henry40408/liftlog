-- Add share_token column for sharing workouts publicly
ALTER TABLE workout_sessions ADD COLUMN share_token TEXT;

-- Create unique index for share_token (only for non-null values)
CREATE UNIQUE INDEX IF NOT EXISTS idx_workout_sessions_share_token
    ON workout_sessions(share_token) WHERE share_token IS NOT NULL;
