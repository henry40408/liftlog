-- Seed default exercises
INSERT OR IGNORE INTO exercises (id, name, category, muscle_group, equipment, is_default, user_id) VALUES
-- Chest
('ex-bench-press', 'Bench Press', 'chest', 'Pectoralis Major', 'Barbell', 1, NULL),
('ex-incline-bench', 'Incline Bench Press', 'chest', 'Upper Chest', 'Barbell', 1, NULL),
('ex-dumbbell-press', 'Dumbbell Press', 'chest', 'Pectoralis Major', 'Dumbbell', 1, NULL),
('ex-chest-fly', 'Chest Fly', 'chest', 'Pectoralis Major', 'Dumbbell', 1, NULL),
('ex-push-up', 'Push Up', 'chest', 'Pectoralis Major', 'Bodyweight', 1, NULL),

-- Back
('ex-deadlift', 'Deadlift', 'back', 'Erector Spinae', 'Barbell', 1, NULL),
('ex-barbell-row', 'Barbell Row', 'back', 'Latissimus Dorsi', 'Barbell', 1, NULL),
('ex-pull-up', 'Pull Up', 'back', 'Latissimus Dorsi', 'Bodyweight', 1, NULL),
('ex-lat-pulldown', 'Lat Pulldown', 'back', 'Latissimus Dorsi', 'Cable', 1, NULL),
('ex-seated-row', 'Seated Row', 'back', 'Rhomboids', 'Cable', 1, NULL),

-- Legs
('ex-squat', 'Squat', 'legs', 'Quadriceps', 'Barbell', 1, NULL),
('ex-leg-press', 'Leg Press', 'legs', 'Quadriceps', 'Machine', 1, NULL),
('ex-romanian-deadlift', 'Romanian Deadlift', 'legs', 'Hamstrings', 'Barbell', 1, NULL),
('ex-leg-curl', 'Leg Curl', 'legs', 'Hamstrings', 'Machine', 1, NULL),
('ex-leg-extension', 'Leg Extension', 'legs', 'Quadriceps', 'Machine', 1, NULL),
('ex-calf-raise', 'Calf Raise', 'legs', 'Gastrocnemius', 'Machine', 1, NULL),
('ex-lunges', 'Lunges', 'legs', 'Quadriceps', 'Dumbbell', 1, NULL),

-- Shoulders
('ex-overhead-press', 'Overhead Press', 'shoulders', 'Deltoids', 'Barbell', 1, NULL),
('ex-lateral-raise', 'Lateral Raise', 'shoulders', 'Lateral Deltoid', 'Dumbbell', 1, NULL),
('ex-front-raise', 'Front Raise', 'shoulders', 'Anterior Deltoid', 'Dumbbell', 1, NULL),
('ex-rear-delt-fly', 'Rear Delt Fly', 'shoulders', 'Posterior Deltoid', 'Dumbbell', 1, NULL),
('ex-face-pull', 'Face Pull', 'shoulders', 'Posterior Deltoid', 'Cable', 1, NULL),

-- Arms
('ex-barbell-curl', 'Barbell Curl', 'arms', 'Biceps', 'Barbell', 1, NULL),
('ex-dumbbell-curl', 'Dumbbell Curl', 'arms', 'Biceps', 'Dumbbell', 1, NULL),
('ex-hammer-curl', 'Hammer Curl', 'arms', 'Brachialis', 'Dumbbell', 1, NULL),
('ex-tricep-pushdown', 'Tricep Pushdown', 'arms', 'Triceps', 'Cable', 1, NULL),
('ex-skull-crusher', 'Skull Crusher', 'arms', 'Triceps', 'Barbell', 1, NULL),
('ex-tricep-dip', 'Tricep Dip', 'arms', 'Triceps', 'Bodyweight', 1, NULL),

-- Core
('ex-plank', 'Plank', 'core', 'Rectus Abdominis', 'Bodyweight', 1, NULL),
('ex-crunch', 'Crunch', 'core', 'Rectus Abdominis', 'Bodyweight', 1, NULL),
('ex-leg-raise', 'Leg Raise', 'core', 'Lower Abs', 'Bodyweight', 1, NULL),
('ex-russian-twist', 'Russian Twist', 'core', 'Obliques', 'Bodyweight', 1, NULL),
('ex-cable-crunch', 'Cable Crunch', 'core', 'Rectus Abdominis', 'Cable', 1, NULL);
