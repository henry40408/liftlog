Feature: Workout lifecycle

  The core training journal: start a session, log a set, fix a set,
  remove a set or the whole session, and edit the session metadata.

  Scenario: Starting a workout puts it on the workouts list
    Given I am logged in as "lifter"
    When I start a new workout for today
    Then I am on the workout detail page
    And the workout I created is listed on the workouts page

  Scenario: Logging a set into a workout shows it on the detail page
    Given I am logged in as "lifter"
    And I have an exercise in category "chest"
    And I have a workout
    When I log a set of 100 kg for 5 reps using the exercise I created
    Then I see my set logged at 100 kg for 5 reps

  Scenario: Editing a set updates the weight and reps
    Given I am logged in as "lifter"
    And I have an exercise in category "shoulders"
    And I have a workout with a set of 50 kg for 8 reps
    When I edit my set to 60 kg for 6 reps
    Then I see my set logged at 60 kg for 6 reps

  Scenario: Deleting a single set leaves the workout intact
    Given I am logged in as "lifter"
    And I have an exercise in category "arms"
    And I have a workout with a set of 20 kg for 12 reps
    When I delete my set
    Then my set is no longer shown on the workout
    And I am on the workout detail page

  Scenario: Editing a workout updates its date and notes
    Given I am logged in as "lifter"
    And I have a workout
    When I edit the workout to date "2024-03-14" with notes "Heavy day"
    Then the workout detail shows date "2024-03-14" and notes "Heavy day"

  Scenario: Deleting a workout removes it from the list
    Given I am logged in as "lifter"
    And I have a workout
    When I delete the workout
    Then the workout I deleted is not listed on the workouts page
