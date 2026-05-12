Feature: Stats pages

  Scenario: /stats renders for a logged-in user
    Given I am logged in as "lifter"
    And I have a workout
    Then I see the stats overview

  Scenario: /stats/exercise/{id} renders for an exercise that has logs
    Given I am logged in as "lifter"
    And I have an exercise in category "back"
    And I have a workout with a set of 60 kg for 5 reps
    Then I see exercise-specific stats for the exercise I created

  Scenario: /stats/prs lists the PR after logging a new set
    Given I am logged in as "lifter"
    And I have an exercise in category "legs"
    And I have a workout
    When I log a set of 100 kg for 5 reps using the exercise I created
    Then the PR list shows my exercise
