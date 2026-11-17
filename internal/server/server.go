// Package server wires LiftLog's routes, middleware and handlers together.
//
// Handlers live in handlers_*.go files as methods on *App. Request flow,
// outermost first: HSTS, baseline headers, CSRF guard, session, mux.
package server

import (
	"crypto/rand"
	"database/sql"
	"fmt"
	"log/slog"
	"net/netip"
	"time"

	"github.com/henry40408/liftlog/internal/audit"
	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/config"
	"github.com/henry40408/liftlog/internal/repo"
)

// Throttles are the in-memory abuse limits. They live in memory on purpose; a
// restart resetting them is accepted.
type Throttles struct {
	// Login is keyed by client IP and applies to POST /auth/login.
	Login *auth.RateLimiter[netip.Addr]
	// LoginBackoff is the per-account delay, keyed by the submitted username.
	LoginBackoff *auth.FailureBackoff[string]
	// SensitiveAction covers password re-checks (password change, admin
	// promote and delete), keyed by user id so a stolen session cannot rotate
	// IPs. One budget is shared across those routes.
	SensitiveAction *auth.RateLimiter[string]
	// Setup is keyed by client IP and applies to POST /auth/setup.
	Setup *auth.RateLimiter[netip.Addr]
}

// NewThrottles returns the production limits.
func NewThrottles() Throttles {
	return Throttles{
		Login:           auth.NewRateLimiter[netip.Addr](5, time.Minute),
		LoginBackoff:    auth.NewFailureBackoff[string](auth.LoginBackoffSchedule()),
		SensitiveAction: auth.NewRateLimiter[string](5, 15*time.Minute),
		Setup:           auth.NewRateLimiter[netip.Addr](5, 15*time.Minute),
	}
}

// Options is everything New needs from the outside.
type Options struct {
	// DB is a migrated database. The caller owns and closes it.
	DB *sql.DB
	// Config supplies the proxy, cookie and HSTS settings.
	Config config.Config
	// Logger receives the audit events and the server's own warnings; nil
	// discards them.
	Logger *slog.Logger
	// SetupToken gates first-run setup; nil means no token is active.
	SetupToken *auth.SetupToken
	// Throttles overrides the production limits; nil means NewThrottles().
	Throttles *Throttles
	// Hasher is shared so password hashing stays bounded; nil means a new one.
	Hasher *auth.Hasher
	// LogSalt is the per-process salt of audit session fingerprints; nil means
	// 32 random bytes. Tests pass a fixed value for stable fingerprints.
	LogSalt []byte
	// Now is the clock; nil means time.Now.
	Now func() time.Time
}

// App is the server state shared by every handler.
type App struct {
	users     *repo.Users
	sessions  *repo.Sessions
	exercises *repo.Exercises
	workouts  *repo.Workouts

	cfg        config.Config
	logger     *slog.Logger
	audit      *audit.Logger
	hasher     *auth.Hasher
	setupToken *auth.SetupToken
	throttles  Throttles
	logSalt    []byte
	now        func() time.Time
}

// New builds the app. It does no I/O.
func New(options Options) *App {
	logger := options.Logger
	if logger == nil {
		logger = slog.New(slog.DiscardHandler)
	}
	app := &App{
		users:      repo.NewUsers(options.DB),
		sessions:   repo.NewSessions(options.DB),
		exercises:  repo.NewExercises(options.DB),
		workouts:   repo.NewWorkouts(options.DB),
		cfg:        options.Config,
		logger:     logger,
		audit:      audit.NewLogger(logger),
		hasher:     options.Hasher,
		setupToken: options.SetupToken,
		logSalt:    options.LogSalt,
		now:        options.Now,
	}
	app.applyDefaults(options)
	// Compute the dummy hash now, so the first unknown-username login is not
	// slower than later ones.
	app.hasher.DummyHash()
	return app
}

func (a *App) applyDefaults(options Options) {
	if a.hasher == nil {
		a.hasher = auth.NewHasher()
	}
	if a.setupToken == nil {
		a.setupToken = &auth.SetupToken{}
	}
	if a.now == nil {
		a.now = time.Now
	}
	if a.logSalt == nil {
		a.logSalt = randomSalt()
	}
	a.throttles = NewThrottles()
	if options.Throttles != nil {
		a.throttles = *options.Throttles
	}
}

func randomSalt() []byte {
	salt := make([]byte, 32)
	if _, err := rand.Read(salt); err != nil {
		panic(fmt.Sprintf("generate the audit salt: %v", err))
	}
	return salt
}

// clientSource says how requests resolve their client address.
func (a *App) clientSource() audit.ClientSource {
	return audit.ClientSource{
		TrustedProxyHeader: a.cfg.TrustedProxyHeader,
		TrustedProxies:     a.cfg.TrustedProxies,
	}
}
