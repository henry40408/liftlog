Feature: Personal record badges

  Locks the automatic PR detection: a logged set on a fresh exercise
  must come back from the server tagged as a personal record.

  Scenario: The first set ever logged on an exercise is flagged as a PR
    Given I am logged in as "lifter"
    And I have an exercise in category "legs"
    And I have a workout
    When I log a set of 100 kg for 5 reps using the exercise I created
    Then my set is flagged as a PR
