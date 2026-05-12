Feature: Admin user management

  Locks the admin-only flows on /users: create, promote, delete, and
  the negative path that hides those controls (and refuses the admin
  endpoints) for non-admins.

  Scenario: Admin can create a member user from the UI
    Given I am logged in as "lifter"
    When I create a new user via the admin UI
    Then I see that user listed on the users page

  Scenario: Admin promotes another user to admin
    Given I am logged in as "lifter"
    And another user exists
    When I promote that user to admin
    Then I see that user listed as Admin

  Scenario: Admin deletes a user from the list
    Given I am logged in as "lifter"
    And another user exists
    When I delete that user
    Then I do not see that user on the users page

  Scenario: Non-admin cannot reach admin-only user actions
    Given I am logged in as a fresh non-admin user
    Then I do not see the "+ Add New User" button on the users page
    And visiting "/users/new" returns a 403
