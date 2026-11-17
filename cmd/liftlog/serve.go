package main

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/config"
	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/server"
	"github.com/henry40408/liftlog/internal/version"
	"github.com/henry40408/liftlog/migrations"
)

const (
	shutdownTimeout   = 30 * time.Second
	readHeaderTimeout = 10 * time.Second
	idleTimeout       = 120 * time.Second
	sweepInterval     = time.Hour
)

type serveParams struct {
	cfg    config.Config
	logger *slog.Logger
	// stderr receives the setup token and nothing else.
	stderr io.Writer
}

// serve follows a fixed order. The signal handler goes in before the
// listener, so a signal arriving right after startup still shuts down
// gracefully instead of hitting the default action.
func serve(p serveParams) error {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	pool, err := openDatabase(ctx, p.cfg, p.logger)
	if err != nil {
		return err
	}
	defer func() { _ = pool.Close() }()
	users := repo.NewUsers(pool)
	token, err := announceSetupToken(ctx, users, p.stderr, p.logger)
	if err != nil {
		return err
	}
	logProxyTrust(p.cfg, p.logger)
	logCookieAndHSTS(p.cfg, p.logger)

	app := server.New(server.Options{DB: pool, Config: p.cfg, Logger: p.logger, SetupToken: token})
	listener, err := net.Listen("tcp", p.cfg.Bind.String())
	if err != nil {
		return err
	}
	p.logger.Info("Starting server", "addr", listener.Addr().String(), "version", version.Version())

	httpServer := newHTTPServer(app, p.logger)
	serveErr := make(chan error, 1)
	go func() { serveErr <- httpServer.Serve(listener) }()
	stopSweep := startExpirySweep(pool, p.logger)

	select {
	case err := <-serveErr:
		stopSweep()
		return fmt.Errorf("server stopped: %w", err)
	case <-ctx.Done():
		p.logger.Info("Received shutdown signal, shutting down...")
	}
	// Restore default signal handling, so a second signal force-quits a stuck drain.
	stop()
	shutdownGracefully(httpServer, p.logger, shutdownTimeout)
	// Drained; the sweep stops before the checkpoint so nothing writes after it.
	stopSweep()
	if err := db.Checkpoint(context.Background(), pool); err != nil {
		p.logger.Warn("WAL checkpoint on shutdown failed", "err", err)
	}
	p.logger.Info("Server shut down gracefully")
	return nil
}

func openDatabase(ctx context.Context, cfg config.Config, logger *slog.Logger) (*sql.DB, error) {
	logger.Info("Connecting to database", "database_url", cfg.DatabaseURL)
	pool, err := db.Open(cfg.DatabaseURL)
	if err != nil {
		return nil, err
	}
	if err := migrations.Run(ctx, pool, logger); err != nil {
		_ = pool.Close()
		return nil, err
	}
	return pool, nil
}

// newHTTPServer sets no Read/WriteTimeout: a slow client must not cut off a
// long response.
func newHTTPServer(app *server.App, logger *slog.Logger) *http.Server {
	return &http.Server{
		Handler:           app.Handler(),
		ReadHeaderTimeout: readHeaderTimeout,
		IdleTimeout:       idleTimeout,
		ErrorLog:          slog.NewLogLogger(logger.Handler(), slog.LevelError),
	}
}

// announceSetupToken issues the setup token on an empty install and prints it
// on stderr only. The token must never reach the logger: log pipelines are
// often shipped elsewhere, and the token is all that guards first-run setup.
func announceSetupToken(ctx context.Context, users *repo.Users, stderr io.Writer, logger *slog.Logger) (*auth.SetupToken, error) {
	count, err := users.Count(ctx)
	if err != nil {
		return nil, fmt.Errorf("count users: %w", err)
	}
	token := auth.NewSetupToken(int64(count))
	value, issued := token.Current()
	if !issued {
		return token, nil
	}
	_, _ = fmt.Fprintf(stderr, "Setup token: %s\n", value)
	logger.Info(`Setup token issued: see the line starting with "Setup token" on stderr`)
	return token, nil
}

func shutdownGracefully(httpServer *http.Server, logger *slog.Logger, timeout time.Duration) {
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	err := httpServer.Shutdown(ctx)
	if err == nil {
		return
	}
	logger.Error("graceful shutdown failed, closing connections", "err", err)
	if closeErr := httpServer.Close(); closeErr != nil && !errors.Is(closeErr, net.ErrClosed) {
		logger.Error("failed to close the server", "err", closeErr)
	}
}

func logProxyTrust(cfg config.Config, logger *slog.Logger) {
	switch {
	case cfg.TrustedProxyHeader == config.TrustNoHeader:
		logger.Warn("LIFTLOG_TRUSTED_PROXY_HEADER is not set; forwarding headers are ignored and the TCP peer is used for login rate limiting — behind a reverse proxy every client shares one bucket. Set it to x-forwarded-for or x-real-ip once your proxy is configured to overwrite that header.")
	case len(cfg.TrustedProxies) == 0:
		logger.Info("trusting forwarding header from loopback peers only; set LIFTLOG_TRUSTED_PROXIES if the proxy runs on another host or in another container",
			"header", proxyHeaderName(cfg.TrustedProxyHeader))
	default:
		logger.Info("trusting forwarding header from configured proxies",
			"header", proxyHeaderName(cfg.TrustedProxyHeader), "proxies", cfg.TrustedProxies)
	}
}

func proxyHeaderName(header config.TrustedProxyHeader) string {
	if header == config.TrustXForwardedFor {
		return "x-forwarded-for"
	}
	return "x-real-ip"
}

func logCookieAndHSTS(cfg config.Config, logger *slog.Logger) {
	// warn: liftlog cannot detect HTTPS, and serving it with this off silently
	// drops `Secure` and `__Host-`. Plain-HTTP installs can ignore it.
	if cfg.CookieSecure {
		logger.Info("session cookie Secure attribute is enabled")
	} else {
		logger.Warn("LIFTLOG_COOKIE_SECURE is off: the session cookie is sent without `Secure` and without the `__Host-` prefix, so it will travel over plain HTTP and can be overwritten by a sibling subdomain. Set LIFTLOG_COOKIE_SECURE=true if this deployment is served over HTTPS.")
	}
	if cfg.HSTSMaxAge > 0 {
		logger.Info("HSTS enabled", "max_age", cfg.HSTSMaxAge, "include_subdomains", cfg.HSTSIncludeSubdomains)
	}
}
