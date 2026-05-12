Feature: First-time setup

  On a fresh install the login route should funnel the very first user
  into account creation, and the setup form should refuse weak
  passwords rather than silently creating an insecure admin.

  Scenario: /auth/login redirects to setup when no users exist
    When I visit "/auth/login"
    Then I see the setup page

  Scenario: Setup rejects passwords shorter than 6 characters
    When I submit the setup form with username "tiny" and password "abc"
    Then I see the setup error "Password must be at least 6 characters"
