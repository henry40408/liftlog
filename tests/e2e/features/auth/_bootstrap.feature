Feature: First-time setup

  On a fresh install the login route should funnel the very first user
  into account creation rather than presenting a login form.

  Scenario: /auth/login redirects to setup when no users exist
    When I visit "/auth/login"
    Then I see the setup page
