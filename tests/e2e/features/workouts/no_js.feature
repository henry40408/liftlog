Feature: Destructive actions without JavaScript

  Every destructive action is a link to a server-rendered confirmation page.
  With scripts on, base.html intercepts the click and asks in a dialog
  instead — every other scenario exercises that path. This one turns scripts
  off, which is the case the interstitial exists for: the old
  `onsubmit="return confirm(...)"` guard simply did not run there, so the
  workout and every set in it went on the first click, unannounced.

  Scenario: Deleting a workout with scripts off asks on a page first
    Given I am logged in as "lifter"
    And I have an exercise in category "back"
    And I have a workout with a set of 80 kg for 5 reps
    Then deleting that workout without JavaScript asks for confirmation first
