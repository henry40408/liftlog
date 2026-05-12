Feature: Managing exercises

  Scenario: A newly created exercise appears on the exercises page
    Given I am logged in as "lifter"
    When I create a new exercise in category "legs"
    Then the exercise I created is listed on the exercises page
