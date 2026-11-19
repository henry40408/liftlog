package harness

import (
	"fmt"
	"io"
	"net/http"
)

// Response is one HTTP response, reduced to what the steps assert on.
type Response struct {
	// Status is the final status, after any redirects.
	Status int
	Body   string
}

// Get requests path as the holder of the session cookie, or as a guest when
// it is empty. Redirects are followed.
//
// This is for what the browser cannot show: status codes, and guests.
func (s *Server) Get(path, session string) (Response, error) {
	request, err := http.NewRequest(http.MethodGet, s.URL(path), nil)
	if err != nil {
		return Response{}, err
	}
	if session != "" {
		request.AddCookie(&http.Cookie{Name: "session", Value: session})
	}
	response, err := http.DefaultClient.Do(request)
	if err != nil {
		return Response{}, fmt.Errorf("requesting %s: %w", path, err)
	}
	defer func() { _ = response.Body.Close() }()
	body, err := io.ReadAll(response.Body)
	if err != nil {
		return Response{}, fmt.Errorf("reading the body of %s: %w", path, err)
	}
	return Response{Status: response.StatusCode, Body: string(body)}, nil
}
