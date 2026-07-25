-- Clear historical orphan rows before PRAGMA foreign_keys enforcement is
-- switched on. Enforcement was never enabled (src/db.rs's with_init never set
-- the pragma), so the ON DELETE CASCADE declarations in earlier migrations
-- never fired and deleting users or workout sessions left rows behind.
-- Children are deleted before parents so the deletion order cannot itself
-- create new orphans. Idempotent: deletes 0 rows on a clean database.
DELETE FROM workout_logs WHERE session_id NOT IN (SELECT id FROM workout_sessions);
DELETE FROM workout_logs WHERE exercise_id NOT IN (SELECT id FROM exercises);
DELETE FROM sessions WHERE user_id NOT IN (SELECT id FROM users);
DELETE FROM workout_sessions WHERE user_id NOT IN (SELECT id FROM users);
DELETE FROM exercises WHERE user_id NOT IN (SELECT id FROM users);
