Feature: Dashboard summary

  Scenario: A newly created workout appears in the dashboard's Recent Workouts
    Given I am logged in as a fresh non-admin user
    When I start a new workout for today
    Then the dashboard lists the workout I created in Recent Workouts

  Scenario: "This Week" count reflects a freshly logged workout
    Given I am logged in as a fresh non-admin user
    Then the dashboard "This Week" count is 0
    When I start a new workout for today
    Then the dashboard "This Week" count is 1
