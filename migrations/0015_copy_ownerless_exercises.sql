-- Databases created before 0.4 keep the exercises that version shipped, owned by
-- nobody, and sets logged against them. The app only works with owned exercises,
-- so each user who logged one gets a copy and their sets move to it. The originals
-- stay: personal_records still refers to them, and deleting one would delete those
-- rows. Sets whose session has no user have nobody to give a copy to.
INSERT INTO exercises (id, name, category, user_id)
SELECT DISTINCT e.id || '-' || u.id, e.name, e.category, u.id
FROM workout_logs l
JOIN exercises e ON e.id = l.exercise_id
JOIN workout_sessions s ON s.id = l.session_id
JOIN users u ON u.id = s.user_id
WHERE e.user_id IS NULL;

UPDATE workout_logs
SET exercise_id = exercise_id || '-' ||
    (SELECT user_id FROM workout_sessions WHERE id = workout_logs.session_id)
WHERE exercise_id IN (SELECT id FROM exercises WHERE user_id IS NULL)
  AND session_id IN (SELECT s.id FROM workout_sessions s JOIN users u ON u.id = s.user_id);
