Feature: Workout visibility and not-found behaviour

  Locks the data-isolation contract: a workout belongs to the user
  who logged it; other users should neither see it on their list nor
  be able to navigate to it.

  Scenario: Another user cannot see my workout in their list
    Given I am logged in as "lifter"
    And I have a workout
    When I switch to a fresh non-admin user
    Then I do not see the workout I created on the workouts page

  Scenario: Another user gets a 404 when visiting my workout
    Given I am logged in as "lifter"
    And I have a workout
    When I switch to a fresh non-admin user
    Then visiting the workout I created returns a 404

  Scenario: A fresh user sees the empty state on /workouts
    Given I am logged in as a fresh non-admin user
    Then I see the workouts empty state

  Scenario: Visiting a nonexistent workout returns 404
    Given I am logged in as "lifter"
    Then visiting "/workouts/00000000-0000-0000-0000-000000000000" returns a 404
