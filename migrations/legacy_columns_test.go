package migrations

import (
	"database/sql"
	"path/filepath"
	"slices"
	"testing"
)

// addExercise is the statement the app runs to create one.
const addExercise = "INSERT INTO exercises (id, name, category, user_id) VALUES (?, 'Lunge', 'legs', 'u1')"

// createPre04Database fills pool with a database created before 0.4, as releases
// up to 0.31 left it.
func createPre04Database(t *testing.T, pool *sql.DB) {
	t.Helper()
	mustExec(t, pool, release031Schema)
	mustExec(t, pool, pre04Tables)
	// The release031 rows without those for the replaced tables.
	mustExec(t, pool, `INSERT INTO _migrations (name) VALUES ('`+baselineRecord+`');
		INSERT INTO users VALUES ('u1', 'root', 'hash', '2026-02-01 00:00:00', 'admin', 'Asia/Taipei');
		INSERT INTO workout_sessions VALUES ('w1', 'u1', '2026-02-01', 'notes', '2026-02-01 00:00:00', NULL, NULL);`)
	mustExec(t, pool, pre04Rows)
}

// throughRelaxedColumn stops after the file that relaxes muscle_group, before any
// later one changes rows or announces a schema change.
func throughRelaxedColumn(t *testing.T) []migration {
	t.Helper()
	return shippedMigrations(t)[:2]
}

// referencingRows is every row of exercises and of the tables that point at it.
func referencingRows(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	return queryStrings(t, pool, `
		SELECT concat_ws('|', 'exercise', id, name, category, muscle_group, equipment, is_default, user_id) FROM exercises
		UNION ALL
		SELECT concat_ws('|', 'log', id, session_id, exercise_id, set_number, reps, weight, rpe, is_pr, created_at)
		FROM workout_logs
		UNION ALL
		SELECT concat_ws('|', 'record', id, user_id, exercise_id, record_type, value, achieved_at) FROM personal_records
		ORDER BY 1`)
}

func TestADatabaseCreatedBefore04AcceptsNewExercises(t *testing.T) {
	pool := openMemory(t)
	createPre04Database(t, pool)
	if _, err := pool.ExecContext(ctx, addExercise, "before"); err == nil {
		t.Fatal("the old schema accepted an exercise without muscle_group; the fixture proves nothing")
	}
	rows := referencingRows(t, pool)

	applyTo(t, pool, throughRelaxedColumn(t))

	if got := referencingRows(t, pool); !slices.Equal(got, rows) {
		t.Errorf("data changed\n got: %q\nwant: %q", got, rows)
	}
	_, err := pool.ExecContext(ctx, addExercise, "after")
	noError(t, err)
	for query, want := range map[string]string{
		"PRAGMA integrity_check":                                                   "ok",
		"SELECT count(*) FROM pragma_foreign_key_check":                            "0",
		"SELECT writable_schema FROM pragma_writable_schema":                       "0",
		"SELECT count(*) FROM sqlite_master WHERE name = '_schema_reload'":         "0",
		`SELECT "notnull" FROM pragma_table_info('exercises') WHERE name = 'name'`: "1",
	} {
		if got := queryStrings(t, pool, query); !slices.Equal(got, []string{want}) {
			t.Errorf("%s = %q, want %q", query, got, want)
		}
	}
	// The foreign keys into exercises still hold.
	if _, err := pool.ExecContext(ctx, "DELETE FROM exercises WHERE id = 'e1'"); err == nil {
		t.Error("an exercise with logged sets was deleted")
	}
}

// A connection that read the schema before the migration must not keep refusing.
func TestOtherConnectionsSeeTheRelaxedColumn(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pre04.db")
	other, pool := openFile(t, path), openFile(t, path)
	createPre04Database(t, pool)
	// The baseline changes the schema itself, which would hide a later change that
	// goes unannounced.
	applyTo(t, pool, onlyBaseline(t))
	if _, err := other.ExecContext(ctx, addExercise, "before"); err == nil {
		t.Fatal("the old schema accepted an exercise without muscle_group")
	}

	applyTo(t, pool, throughRelaxedColumn(t))

	_, err := other.ExecContext(ctx, addExercise, "after")
	noError(t, err)
}

// A restored `.dump` loses user_version, so the migrations run again.
func TestUpgradingAPre04DatabaseCanRunTwice(t *testing.T) {
	pool := openMemory(t)
	createPre04Database(t, pool)
	noError(t, Run(ctx, pool, discard))
	schema, rows := storedSchema(t, pool), referencingRows(t, pool)
	mustExec(t, pool, "PRAGMA user_version = 0")

	noError(t, Run(ctx, pool, discard))

	if got := storedSchema(t, pool); !slices.Equal(got, schema) {
		t.Errorf("schema changed\n got: %q\nwant: %q", got, schema)
	}
	if got := referencingRows(t, pool); !slices.Equal(got, rows) {
		t.Errorf("data changed\n got: %q\nwant: %q", got, rows)
	}
}

func TestLaterMigrationsLeaveACurrentDatabaseAlone(t *testing.T) {
	for name, create := range map[string]func(*testing.T) *sql.DB{
		"adopted": release031Database,
		"new": func(t *testing.T) *sql.DB {
			pool := openMemory(t)
			applyTo(t, pool, onlyBaseline(t))
			return pool
		},
	} {
		t.Run(name, func(t *testing.T) {
			pool := create(t)
			applyTo(t, pool, onlyBaseline(t))
			schema := storedSchema(t, pool)
			owners, logs := exerciseOwners(t, pool), loggedExercises(t, pool)

			noError(t, Run(ctx, pool, discard))

			if got := storedSchema(t, pool); !slices.Equal(got, schema) {
				t.Errorf("schema changed\n got: %q\nwant: %q", got, schema)
			}
			if got := exerciseOwners(t, pool); !slices.Equal(got, owners) {
				t.Errorf("exercises changed\n got: %q\nwant: %q", got, owners)
			}
			if got := loggedExercises(t, pool); !slices.Equal(got, logs) {
				t.Errorf("logs changed\n got: %q\nwant: %q", got, logs)
			}
		})
	}
}
