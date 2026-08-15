# Tagged so the runner can give these the empty database, before any other
# scenario seeds its admin. None of the three creates an account, so the
# install is still fresh when the second pass starts.
@bootstrap
Feature: First-time setup

  On a fresh install the login route should funnel the very first user
  into account creation, and the setup form should refuse weak
  passwords rather than silently creating an insecure admin.

  Scenario: /auth/login redirects to setup when no users exist
    When I visit "/auth/login"
    Then I see the setup page

  Scenario: Setup rejects passwords shorter than 12 characters
    When I submit the setup form with username "tiny" and password "abc"
    Then I see the setup error "Password must be at least 12 characters"

  Scenario: Setup rejects a long-enough but easily guessed password
    When I submit the setup form with username "tiny" and password "MyPassword12"
    Then I see the setup error "similar to a commonly used password"
