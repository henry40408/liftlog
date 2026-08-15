Feature: Session behaviour

  Locks how the app routes around an authenticated session: the login
  page must not show to someone who already has a session, and signing
  out must drop them back to it.

  Scenario: An already-logged-in user visiting the login page is sent home
    Given I am logged in as "lifter"
    When I visit "/auth/login"
    Then I see the dashboard

  Scenario: Signing out drops me back at the login page
    Given I am logged in as "lifter"
    When I log out
    Then I see the login page
