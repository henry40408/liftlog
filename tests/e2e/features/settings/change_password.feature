Feature: Changing my password

  Scenario: After changing my password I can log in with the new one
    Given a user "pwchange" with password "originalpass" exists
    And I am logged in as "pwchange" with password "originalpass"
    When I change my password from "originalpass" to "newP4ssw0rd"
    Then I see a password-change success message
    When I log out
    And I log in as "pwchange" with password "newP4ssw0rd"
    Then I see the dashboard
