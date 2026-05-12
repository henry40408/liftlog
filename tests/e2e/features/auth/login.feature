Feature: Logging in
  As a returning lifter
  I want to log in with my credentials
  So that I can see my training dashboard

  Scenario: Logging in with valid credentials lands on the dashboard
    Given a user "lifter" with password "barbell-club" exists
    When I log in as "lifter" with password "barbell-club"
    Then I see the dashboard

  Scenario: Logging in with the wrong password shows an error
    Given a user "lifter" with password "barbell-club" exists
    When I log in as "lifter" with password "definitely-not-it"
    Then I see the login error "Invalid username or password"
    And the URL is "/auth/login"
