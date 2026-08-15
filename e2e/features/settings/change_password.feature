Feature: Changing my password

  Scenario: Mismatched new password is rejected
    Given a user "pwmismatch" with password "amber-tractor-lantern" exists
    And I am logged in as "pwmismatch" with password "amber-tractor-lantern"
    When I submit the password form with current "amber-tractor-lantern", new "velvet-harbour-kestrel", confirm "oopsdifferent"
    Then I see a settings error "New passwords do not match"

  Scenario: Wrong current password is rejected
    Given a user "pwwrong" with password "amber-tractor-lantern" exists
    And I am logged in as "pwwrong" with password "amber-tractor-lantern"
    When I submit the password form with current "wrongguess", new "velvet-harbour-kestrel", confirm "velvet-harbour-kestrel"
    Then I see a settings error "Current password is incorrect"

  Scenario: New password shorter than 12 characters is rejected
    Given a user "pwshort" with password "amber-tractor-lantern" exists
    And I am logged in as "pwshort" with password "amber-tractor-lantern"
    When I submit the password form with current "amber-tractor-lantern", new "abc", confirm "abc"
    Then I see a settings error "New password must be at least 12 characters"

  Scenario: After changing my password I can log in with the new one
    Given a user "pwchange" with password "amber-tractor-lantern" exists
    And I am logged in as "pwchange" with password "amber-tractor-lantern"
    When I change my password from "amber-tractor-lantern" to "velvet-harbour-kestrel"
    Then I see a password-change success message
    When I log out
    And I log in as "pwchange" with password "velvet-harbour-kestrel"
    Then I see the dashboard

  Scenario: A long-enough but easily guessed new password is rejected
    Given a user "pwweak" with password "amber-tractor-lantern" exists
    And I am logged in as "pwweak" with password "amber-tractor-lantern"
    When I submit the password form with current "amber-tractor-lantern", new "MyPassword12", confirm "MyPassword12"
    Then I see a settings error "similar to a commonly used password"
