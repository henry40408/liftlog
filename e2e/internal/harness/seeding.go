package harness

import (
	"fmt"
	"net/http"
	"net/http/cookiejar"
	"net/url"
)

// EnsureUser makes sure username exists, creating it if not. It goes through
// the real HTTP endpoints, so policy, hashing and roles apply. Idempotent: a
// repeat /auth/setup redirects and a repeat /users/new re-renders "taken";
// both are ignored. The CSRF guard lets these through because the client
// sends no Sec-Fetch-Site or Origin.
func (s *Server) EnsureUser(username, password string) error {
	s.seeding.Lock()
	defer s.seeding.Unlock()

	// The admin is either the account asked for or the one that creates it.
	if err := s.callSetup(); err != nil {
		return err
	}
	if username == AdminUsername {
		return nil
	}

	client, err := newSeedingClient()
	if err != nil {
		return err
	}
	if err := s.login(client, AdminUsername, Password); err != nil {
		return err
	}
	response, err := client.PostForm(s.URL("/users/new"), url.Values{"username": {username}, "password": {password}})
	if err != nil {
		return fmt.Errorf("creating the user %s: %w", username, err)
	}
	return response.Body.Close()
}

// OpenSecondSession signs username in on a throwaway client, leaving a second
// live session that is not the browser's.
func (s *Server) OpenSecondSession(username, password string) error {
	client, err := newSeedingClient()
	if err != nil {
		return err
	}
	return s.login(client, username, password)
}

// callSetup creates the first user, or bounces off the install that already
// has one.
func (s *Server) callSetup() error {
	client, err := newSeedingClient()
	if err != nil {
		return err
	}
	response, err := client.PostForm(s.URL("/auth/setup"), url.Values{
		"username":    {AdminUsername},
		"password":    {Password},
		"setup_token": {s.SetupToken},
	})
	if err != nil {
		return fmt.Errorf("calling /auth/setup: %w", err)
	}
	defer func() { _ = response.Body.Close() }()
	if response.StatusCode >= 400 {
		return fmt.Errorf("/auth/setup answered %d", response.StatusCode)
	}
	return nil
}

// login expects a redirect: a re-rendered form means the credentials were
// refused.
func (s *Server) login(client *http.Client, username, password string) error {
	response, err := client.PostForm(s.URL("/auth/login"), url.Values{"username": {username}, "password": {password}})
	if err != nil {
		return fmt.Errorf("signing %s in: %w", username, err)
	}
	defer func() { _ = response.Body.Close() }()
	if response.StatusCode < 300 || response.StatusCode >= 400 {
		return fmt.Errorf("signing %s in expected a redirect, got %d", username, response.StatusCode)
	}
	return nil
}

// newSeedingClient keeps cookies (the admin session spans login to
// /users/new) and does not follow redirects (the login's redirect is how
// success is detected).
func newSeedingClient() (*http.Client, error) {
	jar, err := cookiejar.New(nil)
	if err != nil {
		return nil, err
	}
	return &http.Client{
		Jar: jar,
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}, nil
}
