Feature: Logging in
  As a returning lifter
  I want to log in with my credentials
  So that I can see my training dashboard

  Scenario: Logging in with valid credentials lands on the dashboard
    Given a user "lifter" with password "barbell-club" exists
    When I log in as "lifter" with password "barbell-club"
    Then I see the dashboard
