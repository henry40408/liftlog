Feature: Active session management

  Scenario: Logging out other devices leaves just this session
    Given a user "logoutme" with password "barbell-club" exists
    And I have a second session as "logoutme"
    And I am logged in as "logoutme" with password "barbell-club"
    Then the active sessions table has 2 rows
    When I log out all other devices
    Then the active sessions table has 1 row

  Scenario: The current device is marked on the active sessions list
    Given I am logged in as "lifter"
    Then the active sessions table marks my current device
