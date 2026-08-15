Feature: Sharing a workout

  Background:
    Given I am logged in as "lifter"
    And I have an exercise in category "back"
    And I have a workout with a set of 80 kg for 5 reps

  Scenario: Sharing a workout exposes a public link
    When I share the workout
    Then a public share link is shown on the workout page

  Scenario: A guest can open the share URL without logging in
    Given I have shared the workout
    Then a guest can view the workout via the share URL

  Scenario: Revoking a share makes the URL 404 for guests
    Given I have shared the workout
    When I revoke the share
    Then a guest visiting the share URL gets a 404
