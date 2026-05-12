Feature: Authentication wall

  Unauthenticated visitors to private routes must be sent to the login
  page rather than seeing partial UI or errors.

  Background:
    Given a user "lifter" with password "barbell-club" exists

  Scenario: /workouts redirects to login when not authenticated
    When I visit "/workouts"
    Then I see the login page

  Scenario: /settings redirects to login when not authenticated
    When I visit "/settings"
    Then I see the login page
