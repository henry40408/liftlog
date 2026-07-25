-- Clear historical orphan rows before relying on foreign-key enforcement.
-- The bundled SQLite in this build compiles with SQLITE_DEFAULT_FOREIGN_KEYS,
-- so `PRAGMA foreign_keys` is already ON by default on every connection here
-- -- but migration 010's 12-step table rebuild explicitly turns it OFF for
-- its own duration and never restores it, so the single pooled connection
-- that runs migrations returned from 010 with enforcement off. Any delete
-- that ran through that connection between 010 and this migration (or any
-- delete from before 010 existed at all) could have left orphans that the
-- `ON DELETE CASCADE`/`RESTRICT` declarations never caught.
--
-- This migration turns foreign_keys OFF for its own duration too, so its
-- behaviour does not depend on whether 010 ran in the same batch (a fresh
-- database applies every migration in one pass; an existing database at
-- schema >= 010 skips 010 entirely and would otherwise hit this migration
-- with enforcement back on, tripping the `RESTRICT` on workout_logs.exercise_id
-- and aborting startup). PRAGMA foreign_keys is a no-op inside a transaction,
-- and execute_batch does not wrap its statements in an implicit one (010
-- already relies on the same fact), so setting it first here actually takes
-- effect for the statements that follow.
--
-- With enforcement off, deletes cascade to nothing, so parents are deleted
-- before children: deleting a parent first would otherwise create fresh
-- child orphans that still need sweeping afterwards. Idempotent: deletes 0
-- rows on a clean database.
PRAGMA foreign_keys = OFF;

DELETE FROM sessions WHERE user_id NOT IN (SELECT id FROM users);
DELETE FROM workout_sessions WHERE user_id NOT IN (SELECT id FROM users);
DELETE FROM exercises WHERE user_id NOT IN (SELECT id FROM users);
DELETE FROM workout_logs WHERE session_id NOT IN (SELECT id FROM workout_sessions);
DELETE FROM workout_logs WHERE exercise_id NOT IN (SELECT id FROM exercises);
