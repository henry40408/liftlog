Feature: Changing my password

  Scenario: Mismatched new password is rejected
    Given a user "pwmismatch" with password "originalpass" exists
    And I am logged in as "pwmismatch" with password "originalpass"
    When I submit the password form with current "originalpass", new "newP4ssw0rd", confirm "oopsdifferent"
    Then I see a settings error "New passwords do not match"

  Scenario: Wrong current password is rejected
    Given a user "pwwrong" with password "originalpass" exists
    And I am logged in as "pwwrong" with password "originalpass"
    When I submit the password form with current "wrongguess", new "newP4ssw0rd", confirm "newP4ssw0rd"
    Then I see a settings error "Current password is incorrect"

  Scenario: New password shorter than 6 characters is rejected
    Given a user "pwshort" with password "originalpass" exists
    And I am logged in as "pwshort" with password "originalpass"
    When I submit the password form with current "originalpass", new "abc", confirm "abc"
    Then I see a settings error "New password must be at least 6 characters"

  Scenario: After changing my password I can log in with the new one
    Given a user "pwchange" with password "originalpass" exists
    And I am logged in as "pwchange" with password "originalpass"
    When I change my password from "originalpass" to "newP4ssw0rd"
    Then I see a password-change success message
    When I log out
    And I log in as "pwchange" with password "newP4ssw0rd"
    Then I see the dashboard
