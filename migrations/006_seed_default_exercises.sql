-- Seed default exercises
INSERT OR IGNORE INTO exercises (id, name, category, muscle_group, equipment, is_default, user_id) VALUES
-- 胸 (Chest)
('ex-bench-press', '臥推', 'chest', '胸大肌', '槓鈴', 1, NULL),
('ex-incline-bench', '上斜臥推', 'chest', '上胸', '槓鈴', 1, NULL),
('ex-dumbbell-press', '啞鈴臥推', 'chest', '胸大肌', '啞鈴', 1, NULL),
('ex-chest-fly', '飛鳥', 'chest', '胸大肌', '啞鈴', 1, NULL),
('ex-push-up', '伏地挺身', 'chest', '胸大肌', '徒手', 1, NULL),

-- 背 (Back)
('ex-deadlift', '硬舉', 'back', '豎脊肌', '槓鈴', 1, NULL),
('ex-barbell-row', '槓鈴划船', 'back', '闊背肌', '槓鈴', 1, NULL),
('ex-pull-up', '引體向上', 'back', '闊背肌', '徒手', 1, NULL),
('ex-lat-pulldown', '滑輪下拉', 'back', '闊背肌', '繩索', 1, NULL),
('ex-seated-row', '坐姿划船', 'back', '菱形肌', '繩索', 1, NULL),

-- 腿 (Legs)
('ex-squat', '深蹲', 'legs', '股四頭肌', '槓鈴', 1, NULL),
('ex-leg-press', '腿推', 'legs', '股四頭肌', '機械', 1, NULL),
('ex-romanian-deadlift', '羅馬尼亞硬舉', 'legs', '腿後肌群', '槓鈴', 1, NULL),
('ex-leg-curl', '腿彎舉', 'legs', '腿後肌群', '機械', 1, NULL),
('ex-leg-extension', '腿伸展', 'legs', '股四頭肌', '機械', 1, NULL),
('ex-calf-raise', '提踵', 'legs', '腓腸肌', '機械', 1, NULL),
('ex-lunges', '弓箭步', 'legs', '股四頭肌', '啞鈴', 1, NULL),

-- 肩 (Shoulders)
('ex-overhead-press', '肩推', 'shoulders', '三角肌', '槓鈴', 1, NULL),
('ex-lateral-raise', '側平舉', 'shoulders', '三角肌中束', '啞鈴', 1, NULL),
('ex-front-raise', '前平舉', 'shoulders', '三角肌前束', '啞鈴', 1, NULL),
('ex-rear-delt-fly', '反向飛鳥', 'shoulders', '三角肌後束', '啞鈴', 1, NULL),
('ex-face-pull', '面拉', 'shoulders', '三角肌後束', '繩索', 1, NULL),

-- 手臂 (Arms)
('ex-barbell-curl', '槓鈴彎舉', 'arms', '二頭肌', '槓鈴', 1, NULL),
('ex-dumbbell-curl', '啞鈴彎舉', 'arms', '二頭肌', '啞鈴', 1, NULL),
('ex-hammer-curl', '錘式彎舉', 'arms', '肱肌', '啞鈴', 1, NULL),
('ex-tricep-pushdown', '三頭肌下壓', 'arms', '三頭肌', '繩索', 1, NULL),
('ex-skull-crusher', '碎顱者', 'arms', '三頭肌', '槓鈴', 1, NULL),
('ex-tricep-dip', '雙槓撐體', 'arms', '三頭肌', '徒手', 1, NULL),

-- 核心 (Core)
('ex-plank', '棒式', 'core', '腹直肌', '徒手', 1, NULL),
('ex-crunch', '捲腹', 'core', '腹直肌', '徒手', 1, NULL),
('ex-leg-raise', '抬腿', 'core', '下腹', '徒手', 1, NULL),
('ex-russian-twist', '俄羅斯轉體', 'core', '腹斜肌', '徒手', 1, NULL),
('ex-cable-crunch', '繩索捲腹', 'core', '腹直肌', '繩索', 1, NULL);
