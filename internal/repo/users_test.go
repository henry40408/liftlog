package repo

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/models"
)

func newUserFixture(t *testing.T) (*Users, context.Context) {
	t.Helper()
	return NewUsers(setupTestDB(t)), context.Background()
}

func mustCreateUser(t *testing.T, repo *Users, username string, role models.UserRole) models.User {
	t.Helper()
	user, err := repo.Create(context.Background(), NewUser{Username: username, PasswordHash: "hash-" + username, Role: role})
	if err != nil {
		t.Fatalf("create %s: %v", username, err)
	}
	return user
}

func TestUsersCountEmptyDB(t *testing.T) {
	repo, ctx := newUserFixture(t)
	count, err := repo.Count(ctx)
	if err != nil || count != 0 {
		t.Fatalf("Count() = %d, %v; want 0, nil", count, err)
	}
}

func TestUsersCreate(t *testing.T) {
	repo, _ := newUserFixture(t)
	user := mustCreateUser(t, repo, "testuser", models.RoleUser)
	if user.Username != "testuser" || user.Role != models.RoleUser || user.ID == "" {
		t.Errorf("unexpected user %+v", user)
	}
	if user.PasswordHash != "hash-testuser" {
		t.Errorf("hash not stored as given: %q", user.PasswordHash)
	}
}

func TestUsersCreateAdminRole(t *testing.T) {
	repo, _ := newUserFixture(t)
	user := mustCreateUser(t, repo, "admin", models.RoleAdmin)
	if user.Role != models.RoleAdmin {
		t.Errorf("role = %q", user.Role)
	}
}

func TestUsersFindByIDRoundTrips(t *testing.T) {
	repo, ctx := newUserFixture(t)
	created := mustCreateUser(t, repo, "testuser", models.RoleUser)
	found, err := repo.FindByID(ctx, created.ID)
	if err != nil {
		t.Fatal(err)
	}
	if found.ID != created.ID || found.Username != "testuser" || found.PasswordHash != created.PasswordHash {
		t.Errorf("found %+v, want %+v", found, created)
	}
	if found.Timezone != nil {
		t.Errorf("timezone = %v, want nil", *found.Timezone)
	}
	if !found.CreatedAt.Equal(created.CreatedAt) {
		t.Errorf("created_at = %v, want %v", found.CreatedAt, created.CreatedAt)
	}
}

func TestUsersFindByIDNotExists(t *testing.T) {
	repo, ctx := newUserFixture(t)
	if _, err := repo.FindByID(ctx, "nonexistent-id"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("err = %v, want ErrNotFound", err)
	}
}

func TestUsersFindByUsername(t *testing.T) {
	repo, ctx := newUserFixture(t)
	mustCreateUser(t, repo, "findme", models.RoleUser)
	found, err := repo.FindByUsername(ctx, "findme")
	if err != nil || found.Username != "findme" {
		t.Fatalf("FindByUsername = %+v, %v", found, err)
	}
	if _, err := repo.FindByUsername(ctx, "nouser"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("missing user: err = %v, want ErrNotFound", err)
	}
}

// Usernames are case-sensitive by design (README "Out of scope"). If this
// changed, the per-username login backoff would need the same normalisation,
// or each casing would get a fresh counter.
func TestUsernamesAreCaseSensitive(t *testing.T) {
	repo, ctx := newUserFixture(t)
	mustCreateUser(t, repo, "henry", models.RoleUser)

	if _, err := repo.FindByUsername(ctx, "henry"); err != nil {
		t.Fatalf("the exact spelling must resolve: %v", err)
	}
	for _, variant := range []string{"Henry", "HENRY", "hEnRy"} {
		if _, err := repo.FindByUsername(ctx, variant); !errors.Is(err, ErrNotFound) {
			t.Errorf("%s must not resolve to henry, err = %v", variant, err)
		}
	}

	// Case-only duplicates are allowed; the README documents it.
	if _, err := repo.Create(ctx, NewUser{Username: "Henry", PasswordHash: "h", Role: models.RoleUser}); err != nil {
		t.Errorf("a case variant is a distinct account, not a duplicate: %v", err)
	}
}

func TestUsersCreateRejectsDuplicateUsername(t *testing.T) {
	repo, ctx := newUserFixture(t)
	mustCreateUser(t, repo, "dup", models.RoleUser)
	if _, err := repo.Create(ctx, NewUser{Username: "dup", PasswordHash: "h", Role: models.RoleUser}); err == nil {
		t.Fatal("duplicate username accepted")
	}
}

func TestUsersFindAllNewestFirst(t *testing.T) {
	repo, ctx := newUserFixture(t)
	for _, name := range []string{"user1", "user2", "user3"} {
		mustCreateUser(t, repo, name, models.RoleUser)
		time.Sleep(10 * time.Millisecond)
	}

	users, err := repo.FindAll(ctx)
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, user := range users {
		names = append(names, user.Username)
	}
	if len(names) != 3 || names[0] != "user3" || names[1] != "user2" || names[2] != "user1" {
		t.Errorf("order = %v, want newest first", names)
	}
}

func TestUsersCreateFirstAdminOnlyOnEmptyTable(t *testing.T) {
	repo, ctx := newUserFixture(t)

	admin, created, err := repo.CreateFirstAdmin(ctx, "admin", "hash")
	if err != nil || !created {
		t.Fatalf("first admin: created=%v err=%v", created, err)
	}
	if admin.Role != models.RoleAdmin || admin.Username != "admin" {
		t.Errorf("unexpected admin %+v", admin)
	}

	_, created, err = repo.CreateFirstAdmin(ctx, "second", "hash")
	if err != nil || created {
		t.Fatalf("second admin: created=%v err=%v, want false, nil", created, err)
	}
	if count, _ := repo.Count(ctx); count != 1 {
		t.Errorf("count = %d, want 1", count)
	}
}

func TestUsersCreateFirstAdminRaceCreatesOne(t *testing.T) {
	repo, ctx := newUserFixture(t)

	var wg sync.WaitGroup
	results := make(chan bool, 8)
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, created, err := repo.CreateFirstAdmin(ctx, "admin"+string(rune('a'+i)), "hash")
			if err != nil {
				t.Error(err)
			}
			results <- created
		}()
	}
	wg.Wait()
	close(results)

	winners := 0
	for created := range results {
		if created {
			winners++
		}
	}
	if winners != 1 {
		t.Errorf("%d setups succeeded, want exactly 1", winners)
	}
}

func TestUsersDelete(t *testing.T) {
	repo, ctx := newUserFixture(t)
	user := mustCreateUser(t, repo, "deleteuser", models.RoleUser)

	deleted, err := repo.Delete(ctx, user.ID)
	if err != nil || !deleted {
		t.Fatalf("Delete = %v, %v", deleted, err)
	}
	if _, err := repo.FindByID(ctx, user.ID); !errors.Is(err, ErrNotFound) {
		t.Errorf("user still found: %v", err)
	}
	if deleted, _ := repo.Delete(ctx, "nonexistent-id"); deleted {
		t.Error("deleting a missing user reported success")
	}
}

func TestUsersUpdateRole(t *testing.T) {
	repo, ctx := newUserFixture(t)
	user := mustCreateUser(t, repo, "roleuser", models.RoleUser)

	updated, err := repo.UpdateRole(ctx, user.ID, models.RoleAdmin)
	if err != nil || !updated {
		t.Fatalf("UpdateRole = %v, %v", updated, err)
	}
	found, _ := repo.FindByID(ctx, user.ID)
	if found.Role != models.RoleAdmin {
		t.Errorf("role = %q, want admin", found.Role)
	}
	if updated, _ := repo.UpdateRole(ctx, "nonexistent", models.RoleAdmin); updated {
		t.Error("updating a missing user reported success")
	}
}

func TestUsersChangePassword(t *testing.T) {
	repo, ctx := newUserFixture(t)
	user := mustCreateUser(t, repo, "pwuser", models.RoleUser)

	changed, err := repo.ChangePassword(ctx, user.ID, "new-hash")
	if err != nil || !changed {
		t.Fatalf("ChangePassword = %v, %v", changed, err)
	}
	found, _ := repo.FindByUsername(ctx, "pwuser")
	if found.PasswordHash != "new-hash" {
		t.Errorf("hash = %q, want new-hash", found.PasswordHash)
	}
	if changed, _ := repo.ChangePassword(ctx, "nonexistent", "x"); changed {
		t.Error("changing a missing user's password reported success")
	}
}

func TestUsersTimezone(t *testing.T) {
	repo, ctx := newUserFixture(t)
	user := mustCreateUser(t, repo, "tzuser", models.RoleUser)

	if tz, err := repo.FindTimezone(ctx, user.ID); err != nil || tz != nil {
		t.Fatalf("fresh user timezone = %v, %v; want nil", tz, err)
	}
	if tz, err := repo.FindTimezone(ctx, "missing"); err != nil || tz != nil {
		t.Fatalf("missing user timezone = %v, %v; want nil", tz, err)
	}

	if changed, err := repo.SetTimezone(ctx, user.ID, "Asia/Taipei"); err != nil || !changed {
		t.Fatalf("SetTimezone = %v, %v", changed, err)
	}
	tz, _ := repo.FindTimezone(ctx, user.ID)
	if tz == nil || *tz != "Asia/Taipei" {
		t.Errorf("timezone = %v", tz)
	}
	found, _ := repo.FindByID(ctx, user.ID)
	if found.Timezone == nil || *found.Timezone != "Asia/Taipei" {
		t.Errorf("User.Timezone = %v", found.Timezone)
	}
	if changed, _ := repo.SetTimezone(ctx, "missing", "UTC"); changed {
		t.Error("setting a missing user's timezone reported success")
	}
}
