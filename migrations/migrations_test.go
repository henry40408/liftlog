package migrations

import (
	"bytes"
	"context"
	"database/sql"
	"errors"
	"io"
	"log/slog"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"testing/fstest"
	"time"

	"github.com/henry40408/liftlog/internal/db"
)

// baselineVersion is the schema version the baseline file creates. It stays the
// same when later migrations are added.
const baselineVersion = 13

var (
	ctx     = context.Background()
	discard = slog.New(slog.NewTextHandler(io.Discard, nil))
)

func openMemory(t *testing.T) *sql.DB {
	t.Helper()
	pool, err := db.OpenMemory()
	noError(t, err)
	t.Cleanup(func() { _ = pool.Close() })
	return pool
}

// openFile opens a database file as the server does, without migrating it.
func openFile(t *testing.T, path string) *sql.DB {
	t.Helper()
	pool, err := db.Open("sqlite:" + path)
	noError(t, err)
	t.Cleanup(func() { _ = pool.Close() })
	return pool
}

func noError(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

func mustExec(t *testing.T, pool *sql.DB, script string) {
	t.Helper()
	if _, err := pool.ExecContext(ctx, script); err != nil {
		t.Fatalf("%s: %v", script, err)
	}
}

// release031Database is a database as releases up to 0.31 left it.
func release031Database(t *testing.T) *sql.DB {
	t.Helper()
	pool := openMemory(t)
	mustExec(t, pool, release031Schema)
	mustExec(t, pool, release031Rows)
	return pool
}

// queryStrings reads a query that yields one text column.
func queryStrings(t *testing.T, pool *sql.DB, query string) []string {
	t.Helper()
	rows, err := pool.QueryContext(ctx, query)
	noError(t, err)
	defer func() { _ = rows.Close() }()
	var all []string
	for rows.Next() {
		var value string
		noError(t, rows.Scan(&value))
		all = append(all, value)
	}
	noError(t, rows.Err())
	return all
}

// storedSchema is every statement as SQLite keeps it.
func storedSchema(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	return queryStrings(t, pool, "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
}

// describedSchema lists columns, foreign keys and indexes, so a schema written in
// one piece compares with one that ALTER TABLE grew.
func describedSchema(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	return queryStrings(t, pool, `
		SELECT concat_ws('|', 'column', m.name, c.cid, c.name, c.type, c."notnull", ifnull(c.dflt_value, '-'), c.pk)
		FROM sqlite_master m, pragma_table_info(m.name) c WHERE m.type = 'table'
		UNION ALL
		SELECT concat_ws('|', 'foreign key', m.name, f."from", f."table", f."to", f.on_update, f.on_delete)
		FROM sqlite_master m, pragma_foreign_key_list(m.name) f WHERE m.type = 'table'
		UNION ALL
		SELECT concat_ws('|', 'index', m.name, i.name, i."unique", i.origin, i.partial,
			x.seqno, ifnull(x.name, '-'), x."desc", x.key)
		FROM sqlite_master m, pragma_index_list(m.name) i, pragma_index_xinfo(i.name) x WHERE m.type = 'table'
		UNION ALL
		SELECT concat_ws('|', 'index sql', replace(replace(sql, char(10), ''), ' ', ''))
		FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL
		ORDER BY 1`)
}

func trackedRows(t *testing.T, pool *sql.DB) []string {
	t.Helper()
	return queryStrings(t, pool, "SELECT name || ' ' || applied_at FROM _migrations ORDER BY rowid")
}

func schemaVersion(t *testing.T, pool *sql.DB) int64 {
	t.Helper()
	version, err := userVersion(ctx, pool)
	noError(t, err)
	return version
}

func shippedMigrations(t *testing.T) []migration {
	t.Helper()
	shipped, err := loadMigrations(embeddedMigrations)
	noError(t, err)
	return shipped
}

// onlyBaseline keeps a test about the baseline true after later migrations ship.
func onlyBaseline(t *testing.T) []migration {
	t.Helper()
	return shippedMigrations(t)[:1]
}

func applyTo(t *testing.T, pool *sql.DB, migrations []migration) {
	t.Helper()
	noError(t, applyMigrations(ctx, pool, migrations, discard))
}

func requireError(t *testing.T, err error, want string) {
	t.Helper()
	if err == nil || !strings.Contains(err.Error(), want) {
		t.Fatalf("err = %v, want it to say %q", err, want)
	}
}

// The baseline file replaced the migrations up to 013; a lower or higher number
// would skip or repeat it on the databases those left.
func TestTheBaselineComesFirst(t *testing.T) {
	if got := shippedMigrations(t)[0].version; got != baselineVersion {
		t.Fatalf("first migration = %d, want the baseline %d", got, baselineVersion)
	}
}

func TestRunBringsANewDatabaseToTheLatestVersion(t *testing.T) {
	pool := openMemory(t)
	noError(t, Run(ctx, pool, discard))

	shipped := shippedMigrations(t)
	if got, want := schemaVersion(t, pool), shipped[len(shipped)-1].version; got != want {
		t.Errorf("user_version = %d, want %d", got, want)
	}
	records := queryStrings(t, pool, "SELECT name FROM _migrations")
	if !slices.Equal(records, []string{baselineRecord}) {
		t.Errorf("_migrations = %q, want only %q", records, baselineRecord)
	}
	for query, want := range map[string]string{
		"SELECT count(*) FROM pragma_foreign_key_check": "0",
		"SELECT foreign_keys FROM pragma_foreign_keys":  "1",
	} {
		if got := queryStrings(t, pool, query); !slices.Equal(got, []string{want}) {
			t.Errorf("%s = %q, want %q", query, got, want)
		}
	}
}

func TestRunForTestsMigrates(t *testing.T) {
	pool := openMemory(t)
	noError(t, RunForTests(ctx, pool))
	if got := schemaVersion(t, pool); got < baselineVersion {
		t.Fatalf("user_version = %d", got)
	}
}

func TestBaselineBuildsTheSchemaOlderReleasesBuilt(t *testing.T) {
	fresh := openMemory(t)
	applyTo(t, fresh, onlyBaseline(t))

	got := describedSchema(t, fresh)
	want := describedSchema(t, release031Database(t))
	if !slices.Equal(got, want) {
		t.Fatalf("schema differs\n got: %q\nwant: %q", got, want)
	}
}

// A database an older release left has user_version 0. A stale number below the
// baseline, left by some other tool, must not matter either.
func TestRunAdoptsADatabaseOfAnOlderRelease(t *testing.T) {
	for _, stale := range []string{"0", "5", "12"} {
		t.Run("user_version "+stale, func(t *testing.T) {
			pool := release031Database(t)
			mustExec(t, pool, "PRAGMA user_version = "+stale)
			schema, records := storedSchema(t, pool), trackedRows(t, pool)

			applyTo(t, pool, onlyBaseline(t))

			if got := schemaVersion(t, pool); got != baselineVersion {
				t.Errorf("user_version = %d", got)
			}
			if got := storedSchema(t, pool); !slices.Equal(got, schema) {
				t.Errorf("schema changed\n got: %q\nwant: %q", got, schema)
			}
			if got := trackedRows(t, pool); !slices.Equal(got, records) {
				t.Errorf("records changed\n got: %q\nwant: %q", got, records)
			}
			rows := queryStrings(t, pool, `SELECT concat_ws('|', u.username, u.timezone, l.weight, s.token)
				FROM users u, workout_logs l, sessions s`)
			if want := []string{"root|Asia/Taipei|102.5|t1"}; !slices.Equal(rows, want) {
				t.Errorf("data = %q, want %q", rows, want)
			}
		})
	}
}

// A restored `.dump` loses user_version.
func TestBaselineReplaysOverADatabaseThatLostItsVersion(t *testing.T) {
	fresh := func(t *testing.T) *sql.DB {
		pool := openMemory(t)
		applyTo(t, pool, onlyBaseline(t))
		return pool
	}
	for name, create := range map[string]func(*testing.T) *sql.DB{
		"adopted": release031Database,
		"new":     fresh,
	} {
		t.Run(name, func(t *testing.T) {
			pool := create(t)
			applyTo(t, pool, onlyBaseline(t))
			records := trackedRows(t, pool)
			mustExec(t, pool, "PRAGMA user_version = 0")

			applyTo(t, pool, onlyBaseline(t))

			if got := schemaVersion(t, pool); got != baselineVersion {
				t.Errorf("user_version = %d", got)
			}
			if got := trackedRows(t, pool); !slices.Equal(got, records) {
				t.Errorf("records changed\n got: %q\nwant: %q", got, records)
			}
		})
	}
}

func TestRunRefusesADatabaseItCannotUpgradeAndLeavesItAlone(t *testing.T) {
	changed := func(change string) func(*testing.T) *sql.DB {
		return func(t *testing.T) *sql.DB {
			pool := release031Database(t)
			mustExec(t, pool, change)
			return pool
		}
	}
	holding := func(script string) func(*testing.T) *sql.DB {
		return func(t *testing.T) *sql.DB {
			pool := openMemory(t)
			mustExec(t, pool, script)
			return pool
		}
	}
	const noRecord = "no record of migration"
	tests := map[string]struct {
		create func(*testing.T) *sql.DB
		want   string
	}{
		"one migration short":  {changed("DELETE FROM _migrations WHERE name = '" + baselineRecord + "'"), noRecord},
		"of another program":   {holding("CREATE TABLE notes (body TEXT)"), noRecord},
		"with only a view":     {holding("CREATE VIEW one AS SELECT 1"), noRecord},
		"with unreadable rows": {holding("CREATE TABLE _migrations (id INTEGER)"), "read old migration records"},
		"from a newer release": {changed("PRAGMA user_version = 1000"), "newer than"},
	}
	for name, test := range tests {
		t.Run(name, func(t *testing.T) {
			pool := test.create(t)
			version, schema := schemaVersion(t, pool), storedSchema(t, pool)

			requireError(t, Run(ctx, pool, discard), test.want)

			if got := schemaVersion(t, pool); got != version {
				t.Errorf("user_version = %d, was %d", got, version)
			}
			if got := storedSchema(t, pool); !slices.Equal(got, schema) {
				t.Errorf("schema changed\n got: %q\nwant: %q", got, schema)
			}
		})
	}
}

func TestLaterMigrationsApplyOnceAndInOrder(t *testing.T) {
	pool := openMemory(t)
	later := append(onlyBaseline(t),
		migration{version: 14, sql: "CREATE TABLE later (id INTEGER)"},
		migration{version: 15, sql: "ALTER TABLE later ADD COLUMN name TEXT"})

	// Neither statement can run twice.
	for range 2 {
		applyTo(t, pool, later)
		if got := schemaVersion(t, pool); got != 15 {
			t.Fatalf("user_version = %d", got)
		}
	}
}

// From the baseline on, user_version alone counts: the old records are only for
// databases below it.
func TestLaterMigrationsDoNotNeedTheOldRecords(t *testing.T) {
	pool := openMemory(t)
	mustExec(t, pool, "CREATE TABLE kept (id INTEGER); PRAGMA user_version = 13")

	applyTo(t, pool, append(onlyBaseline(t), migration{version: 14, sql: "CREATE TABLE later (id INTEGER)"}))

	if got := schemaVersion(t, pool); got != 14 {
		t.Fatalf("user_version = %d", got)
	}
}

func TestApplyMigrationSkipsOneAlreadyApplied(t *testing.T) {
	pool := openMemory(t)
	applyTo(t, pool, onlyBaseline(t))
	done := migration{version: baselineVersion, sql: "this is not SQL"}
	known := versionRange{baseline: baselineVersion, latest: baselineVersion}
	noError(t, applyMigration(ctx, pool, done, known))
}

// A newer release may upgrade the database between the first read and the write lock.
func TestApplyMigrationRefusesADatabaseThatBecameNewer(t *testing.T) {
	pool := openMemory(t)
	mustExec(t, pool, "PRAGMA user_version = 14")
	pending := migration{version: baselineVersion, sql: "this is not SQL"}
	known := versionRange{baseline: baselineVersion, latest: baselineVersion}
	requireError(t, applyMigration(ctx, pool, pending, known), "newer than")
}

func TestAFailedMigrationLeavesNothingBehind(t *testing.T) {
	pool := openMemory(t)
	broken := append(onlyBaseline(t),
		migration{version: 14, sql: "CREATE TABLE done (id INTEGER)"},
		migration{
			version: 15, name: "0015_broken.sql",
			sql: "CREATE TABLE half_done (id INTEGER); INSERT INTO no_such_table VALUES (1);",
		})

	requireError(t, applyMigrations(ctx, pool, broken, discard), "migration 0015_broken.sql")

	if got := queryStrings(t, pool, "SELECT name FROM sqlite_master WHERE name = 'half_done'"); len(got) != 0 {
		t.Error("the half-applied table survived")
	}
	if got := schemaVersion(t, pool); got != 14 {
		t.Errorf("user_version = %d, want the last migration that succeeded", got)
	}
}

func TestAVersionThatCannotBeRecordedRollsTheMigrationBack(t *testing.T) {
	pool := openMemory(t)
	// query_only makes the write of user_version fail after the script ran.
	unrecordable := []migration{{version: 13, sql: "CREATE TABLE unrecorded (id INTEGER); PRAGMA query_only = 1"}}

	requireError(t, applyMigrations(ctx, pool, unrecordable, discard), "set schema version")

	if got := storedSchema(t, pool); len(got) != 0 {
		t.Errorf("the table survived without its version: %q", got)
	}
}

// A script that ends the transaction itself leaves nothing to commit.
func TestAMigrationThatCannotCommitIsReported(t *testing.T) {
	committing := []migration{{version: 13, sql: "COMMIT"}}
	requireError(t, applyMigrations(ctx, openMemory(t), committing, discard), "commit")
}

func TestRunReportsADatabaseItCannotRead(t *testing.T) {
	pool := openMemory(t)
	noError(t, pool.Close())
	requireError(t, Run(ctx, pool, discard), "read schema version")
}

func TestApplyMigrationStopsWhenCancelled(t *testing.T) {
	cancelled, cancel := context.WithCancel(ctx)
	cancel()
	known := versionRange{baseline: baselineVersion, latest: baselineVersion}
	err := applyMigration(cancelled, openMemory(t), onlyBaseline(t)[0], known)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v", err)
	}
}

// An older release may be creating its tables while this one starts. A check made
// before taking the write lock would still see the database as it was before.
func TestTheBaselineCheckWaitsForOtherWriters(t *testing.T) {
	path := filepath.Join(t.TempDir(), "race.db")
	other, pool := openFile(t, path), openFile(t, path)
	tx, err := other.BeginTx(ctx, nil)
	noError(t, err)
	_, err = tx.ExecContext(ctx, "CREATE TABLE users (id TEXT)")
	noError(t, err)

	migrated := make(chan error, 1)
	go func() { migrated <- Run(ctx, pool, discard) }()
	// Long enough for Run to reach the lock, far shorter than its busy_timeout.
	time.Sleep(200 * time.Millisecond)
	noError(t, tx.Commit())

	requireError(t, <-migrated, "no record of migration")
}

// A second server may hold the write lock while this one starts.
func TestAnUpToDateDatabaseNeedsNoWriteLock(t *testing.T) {
	path := filepath.Join(t.TempDir(), "current.db")
	other, pool := openFile(t, path), openFile(t, path)
	noError(t, Run(ctx, pool, discard))
	tx, err := other.BeginTx(ctx, nil)
	noError(t, err)
	defer func() { _ = tx.Rollback() }()

	noError(t, Run(ctx, pool, discard))
}

// Orphans are harmless, so they must not keep the server from starting.
func TestRunWarnsAboutForeignKeyViolationsAndStarts(t *testing.T) {
	pool := release031Database(t)
	mustExec(t, pool, `PRAGMA foreign_keys = OFF;
		INSERT INTO exercises VALUES ('orphan', 'Squat', 'legs', 'nobody');
		PRAGMA foreign_keys = ON`)
	var logged bytes.Buffer

	noError(t, Run(ctx, pool, slog.New(slog.NewTextHandler(&logged, nil))))

	if !strings.Contains(logged.String(), "reported violations after migrations") {
		t.Errorf("log = %s", logged.String())
	}
}

func TestLoadMigrationsReportsAFileItCannotRead(t *testing.T) {
	// A directory named like a migration.
	if _, err := loadMigrations(fstest.MapFS{"0001_directory.sql/nested": {}}); err == nil {
		t.Fatal("the directory was loaded")
	}
}

func TestLoadMigrationsOrdersByVersion(t *testing.T) {
	fsys := fstest.MapFS{
		"0010_late_one.sql":  {Data: []byte("SELECT 10;")},
		"0002_early.sql":     {Data: []byte("SELECT 2;")},
		"2147483647_end.sql": {Data: []byte("SELECT 3;")},
		"notes.txt":          {Data: []byte("ignored")},
	}
	got, err := loadMigrations(fsys)
	noError(t, err)
	want := []migration{
		{2, "0002_early.sql", "SELECT 2;"},
		{10, "0010_late_one.sql", "SELECT 10;"},
		{2147483647, "2147483647_end.sql", "SELECT 3;"},
	}
	if !slices.Equal(got, want) {
		t.Fatalf("migrations = %+v, want %+v", got, want)
	}
}

func TestLoadMigrationsRejectsBadNames(t *testing.T) {
	// user_version holds 1 to 2147483647 here.
	names := []string{"init.sql", "x_init.sql", "0001.sql", "0000_zero.sql", "2147483648_big.sql"}
	for _, name := range names {
		fsys := fstest.MapFS{name: {Data: []byte("SELECT 1;")}}
		if _, err := loadMigrations(fsys); err == nil {
			t.Errorf("%s was accepted", name)
		}
	}
}

func TestLoadMigrationsRejectsDuplicateVersions(t *testing.T) {
	fsys := fstest.MapFS{
		"0001_a.sql":  {Data: []byte("SELECT 1;")},
		"00001_b.sql": {Data: []byte("SELECT 1;")},
	}
	_, err := loadMigrations(fsys)
	requireError(t, err, "share version 1")
}

func TestLoadMigrationsRejectsAnEmptySet(t *testing.T) {
	if _, err := loadMigrations(fstest.MapFS{}); err == nil {
		t.Fatal("no files were accepted")
	}
}
