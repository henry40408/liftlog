@no-js
Feature: Destructive actions without JavaScript

  With scripting off, a Delete link opens a server-rendered confirmation page.
  Opening it changes nothing; only its button acts, and Cancel backs out.

  Scenario: Deleting an exercise goes through the confirmation page
    Given I am logged in as "lifter"
    And I have an exercise in category "arms"
    When I click Delete on my exercise
    Then I am asked to confirm
    And my exercise is still listed on the exercises page
    When I click Delete on my exercise
    And I confirm the action
    Then my exercise is no longer listed on the exercises page

  Scenario: Cancelling the confirmation keeps the exercise
    Given I am logged in as "lifter"
    And I have an exercise in category "back"
    When I click Delete on my exercise
    And I cancel the action
    Then I am on the exercises page
    And my exercise is still listed on the exercises page

  Scenario: Deleting a workout goes through the confirmation page
    Given I am logged in as "lifter"
    And I have a workout
    When I click Delete on the workout
    And I confirm the action
    Then the workout I deleted is not listed on the workouts page
