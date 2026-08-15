Feature: Editing and deleting exercises

  Scenario: Renaming an exercise updates its entry on the list
    Given I am logged in as "lifter"
    And I have an exercise in category "back"
    When I rename my exercise
    Then the exercise I created is listed on the exercises page

  Scenario: Deleting an exercise removes it from the list
    Given I am logged in as "lifter"
    And I have an exercise in category "arms"
    When I delete my exercise
    Then my exercise is no longer listed on the exercises page
