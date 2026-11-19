package main

import (
	"context"
	"database/sql"
	"log/slog"
	"sync"
	"time"

	"github.com/henry40408/liftlog/internal/audit"
	"github.com/henry40408/liftlog/internal/repo"
)

// sweeper retires sessions nobody revisits (the session middleware only
// deletes the stale rows it sees) and expired share tokens.
type sweeper struct {
	sessions *repo.Sessions
	workouts *repo.Workouts
	audit    *audit.Logger
	logger   *slog.Logger
}

func newSweeper(pool *sql.DB, logger *slog.Logger) *sweeper {
	return &sweeper{
		sessions: repo.NewSessions(pool),
		workouts: repo.NewWorkouts(pool),
		audit:    audit.NewLogger(logger),
		logger:   logger,
	}
}

// sweepOnce runs both cleanups independently: neither failure may skip the other.
func (s *sweeper) sweepOnce(ctx context.Context) {
	count, err := s.sessions.CleanupExpired(ctx)
	switch {
	case err != nil:
		s.logger.Warn("session cleanup_expired failed", "err", err)
	case count > 0:
		s.audit.SessionsExpiredSweep(int(count))
	}
	count, err = s.workouts.CleanupExpiredShareTokens(ctx)
	switch {
	case err != nil:
		s.logger.Warn("workout cleanup_expired_share_tokens failed", "err", err)
	case count > 0:
		s.logger.Info("cleared expired workout share tokens", "count", count)
	}
}

// run sweeps once at startup, then every interval until ctx ends. The startup
// pass clears what piled up while the server was down, as the original
// ticker did by firing immediately.
func (s *sweeper) run(ctx context.Context, interval time.Duration) {
	s.sweepOnce(ctx)
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		select {
		case <-ticker.C:
			s.sweepOnce(ctx)
		case <-ctx.Done():
			return
		}
	}
}

// startExpirySweep starts the hourly sweep. The returned stop function cancels
// the sweep and waits for a running pass to return, so the caller can safely
// checkpoint and close the database afterwards. It is safe to call more than
// once.
func startExpirySweep(pool *sql.DB, logger *slog.Logger) (stop func()) {
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() {
		defer close(done)
		newSweeper(pool, logger).run(ctx, sweepInterval)
	}()
	cancelOnce := sync.OnceFunc(cancel)
	return func() {
		cancelOnce()
		<-done
	}
}
