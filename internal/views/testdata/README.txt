HTML goldens captured from the Rust binary (debug build, `cargo build` at the root of
commit 19c79d8) before it is removed.

Captured with:
  GIT_VERSION     v0.30.1-1-g19c79d8   (build.rs `git describe`; appears in every
                                        `?v=` asset query and in the settings page footer)
  capture clock   2026-11-16 ~16:40 UTC (Monday)
  seed            seed.sql, applied with sqlite3 on top of a freshly migrated DB
                  (start the binary once, stop it, `sqlite3 db < seed.sql`)
  seeded server   127.0.0.1:3991, requests carry `Cookie: session=<token>` below
  empty server   127.0.0.1:3992, a separate empty DB for the first-run setup pages
                  (its final state is ../../db/testdata/rust-created.sqlite3)
  ids.json        name -> id map for the seed ("ids", uuid5 of a fixed namespace) and the
                  four seeded session tokens ("tokens"). `ws-aNN` is alice's workout NN days/3
                  before the capture day (a00 is today and empty), `ws-bN` is bob's,
                  `log-<ws>-<exercise>-<set>` and `ex-<user>-<name>` likewise.

Seed users (all passwords: `correct horse battery staple`, the argon2id hash is in seed.sql)
  alice  admin  timezone NULL (browser timezone)   2 login sessions (alice-current, alice-other)
  bob    user   Asia/Taipei                        1 login session
  carol  user   America/New_York                   1 login session, no exercises or workouts
Session tokens are 64 hex chars (ids.json "tokens"); `expires_at` is pinned at created_at + 90
days so a touch only rewrites `last_touched_at`.

The seeded shares: ws-a02 token `seed-share-active-...` (expires 2026-12-16 12:00 UTC),
ws-a05 `seed-share-forever-...` (never expires), ws-a06 `seed-share-expired-...` (expired
2026-11-01, so /shared/<token> is 404).

HTML is minified by maud: one line, no trailing newline. Escaped characters are only
& < > " (apostrophes stay raw); see exercises-edit-escaped and workout-show-notes.

Values that depend on the clock (the Go test must pin its clock to 2026-11-16 16:40 UTC or
normalise them):
  * Every relative window is anchored on today: dashboard / stats-index "This Week" (7 days),
    "This Month" (30 days), the weekly volume; the "PR (1M)" badge and the recent-PR columns
    (logs created within the last 30 days); `/workouts/new` default date
    (alice 2026-11-16 UTC, bob 2026-11-17 in Asia/Taipei).
  * settings-*.golden.html: the "last active" time of the *current* session is the moment of
    the first request that touched it (`2026-11-16T16:40:02...+00:00`; a throttled touch keeps
    it for an hour), which differs between alice / bob / carol. Replace it before comparing.
    The other alice session keeps its seeded 2026-11-10 08:00.
  * `?v=v0.30.1-1-g19c79d8` and the settings footer version: normalise like the comics
    goldens do (it is the build version, not part of the page).
  * `<time>` elements carry the seeded RFC3339 times, which are stable.

Status codes that are not 200, and responses that are not HTML (no golden), are in the tables
below. Plain-text bodies are the literal body. headers.txt has the response headers of the
interesting cases (Set-Cookie, Clear-Site-Data, Cache-Control, CSP, static assets).
Set-Cookie attribute order from the Rust server: `HttpOnly; SameSite=Lax; Path=/; Max-Age=604800`
(removal: `session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0`).

Format of the tables: golden file | request [form body] | user | status

== Seeded server (user = whose session cookie is sent; none = anonymous) ==
login.golden.html | GET /auth/login | none | 200
login-failed.golden.html | POST /auth/login [username=alice&password=wrong] | none | 200
(no golden) anon-redirect-index | GET / | none | 303 | ctype= | location=/auth/login | body=
shared.golden.html | GET /shared/seed-share-active-0000000000000000000000000000000000000000 | none | 200
shared-forever.golden.html | GET /shared/seed-share-forever-000000000000000000000000000000000000000 | none | 200
(no golden) shared-expired | GET /shared/seed-share-expired-000000000000000000000000000000000000000 | none | 404 | ctype=text/plain; charset=utf-8 | location= | body=Shared workout not found
(no golden) shared-unknown | GET /shared/nope | none | 404 | ctype=text/plain; charset=utf-8 | location= | body=Shared workout not found
dashboard.golden.html | GET / | alice | 200
dashboard-timezone-set.golden.html | GET / | bob | 200
dashboard-empty.golden.html | GET / | carol | 200
workouts-list.golden.html | GET /workouts | alice | 200
workouts-list-page2.golden.html | GET /workouts?page=2 | alice | 200
workouts-list-page3.golden.html | GET /workouts?page=3 | alice | 200
workouts-list-page-beyond.golden.html | GET /workouts?page=9 | alice | 200
workouts-list-page-zero.golden.html | GET /workouts?page=0 | alice | 200
workouts-list-empty.golden.html | GET /workouts | carol | 200
workouts-new.golden.html | GET /workouts/new | alice | 200
workouts-new-timezone-set.golden.html | GET /workouts/new | bob | 200
workout-show.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd | alice | 200
workout-show-prefill.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd?prefill=cfec4f55-8e9a-559e-9c6e-bcaee6254f90 | alice | 200
workout-show-prefill-foreign.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd?prefill=8cc802ff-bd30-5526-8da0-4886aa207173 | alice | 200
workout-show-prefill-unknown.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd?prefill=nope | alice | 200
workout-show-prefill-rpe.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd?prefill=a1e7f98d-1592-55db-b31e-b0514388cde2 | alice | 200
workout-show-empty.golden.html | GET /workouts/71cfcccf-b0aa-533b-84e4-55839cfbd941 | alice | 200
workout-show-notes.golden.html | GET /workouts/02782218-b94f-5b6c-9a5c-672691802f7e | alice | 200
workout-show-shared.golden.html | GET /workouts/4592a164-902e-551c-a8e9-a7cf803de5da | alice | 200
workout-show-shared-forever.golden.html | GET /workouts/d5cc92da-73eb-5a82-84bb-98b36109a2fd | alice | 200
workout-show-shared-expired.golden.html | GET /workouts/d832baf8-56b4-5105-9e97-1be229372651 | alice | 200
workout-show-bodyweight.golden.html | GET /workouts/ecec20c9-a65f-5b32-b372-e344852aa532 | alice | 200
(no golden) workout-show-foreign | GET /workouts/c76f5408-51e6-55ce-b637-4c873b32afca | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Workout not found
workout-show-timezone-set.golden.html | GET /workouts/c76f5408-51e6-55ce-b637-4c873b32afca | bob | 200
workout-edit.golden.html | GET /workouts/02782218-b94f-5b6c-9a5c-672691802f7e/edit | alice | 200
workout-edit-no-notes.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd/edit | alice | 200
(no golden) workout-edit-foreign | GET /workouts/c76f5408-51e6-55ce-b637-4c873b32afca/edit | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Workout not found
workout-edit-log.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd/logs/a1e7f98d-1592-55db-b31e-b0514388cde2/edit | alice | 200
workout-edit-log-no-rpe.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd/logs/cfec4f55-8e9a-559e-9c6e-bcaee6254f90/edit | alice | 200
(no golden) workout-edit-log-foreign | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd/logs/8cc802ff-bd30-5526-8da0-4886aa207173/edit | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Log not found
exercises-list.golden.html | GET /exercises | alice | 200
exercises-list-empty.golden.html | GET /exercises | carol | 200
exercises-new.golden.html | GET /exercises/new | alice | 200
exercises-new-error.golden.html | POST /exercises [name=+++&category=legs] | alice | 200
exercises-edit.golden.html | GET /exercises/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea/edit | alice | 200
exercises-edit-escaped.golden.html | GET /exercises/0376c522-0917-5353-b064-fbfc9510d054/edit | alice | 200
exercises-edit-error.golden.html | POST /exercises/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea [name=&category=legs] | alice | 200
(no golden) exercises-edit-foreign | GET /exercises/22a1bd31-38fc-53db-9ce2-9d43243ea063/edit | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Exercise not found
stats-index.golden.html | GET /stats | alice | 200
stats-index-empty.golden.html | GET /stats | carol | 200
stats-prs.golden.html | GET /stats/prs | alice | 200
stats-prs-empty.golden.html | GET /stats/prs | carol | 200
stats-exercise-squat.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea | alice | 200
stats-exercise-squat-top_set-20.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea?metric=top_set&range=20 | alice | 200
stats-exercise-squat-top_set-all.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea?metric=top_set&range=all | alice | 200
stats-exercise-squat-e1rm-20.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea?metric=e1rm&range=20 | alice | 200
stats-exercise-squat-e1rm-all.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea?metric=e1rm&range=all | alice | 200
stats-exercise-squat-volume-20.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea?metric=volume&range=20 | alice | 200
stats-exercise-squat-volume-all.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea?metric=volume&range=all | alice | 200
stats-exercise-squat-unknown-query.golden.html | GET /stats/exercise/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea?metric=bogus&range=bogus | alice | 200
stats-exercise-bench.golden.html | GET /stats/exercise/fbe58868-1aa5-5ab6-ad18-9dd398a49b5b?metric=e1rm | alice | 200
stats-exercise-deadlift.golden.html | GET /stats/exercise/f2efc5aa-2ac8-54b6-af56-933b6c5be8cf?metric=volume | alice | 200
stats-exercise-ohp.golden.html | GET /stats/exercise/9f62366c-2d2b-5ab4-ab90-a9a93d1b08de | alice | 200
stats-exercise-pullup.golden.html | GET /stats/exercise/de28bf00-4cb9-5486-877d-dd7f8f55f010 | alice | 200
stats-exercise-curl.golden.html | GET /stats/exercise/9264ffa1-b941-569f-88ec-e1f12d75ad74 | alice | 200
stats-exercise-plank.golden.html | GET /stats/exercise/fac81715-c51c-5bae-a239-e6359a311d8e | alice | 200
(no golden) stats-exercise-foreign | GET /stats/exercise/22a1bd31-38fc-53db-9ce2-9d43243ea063 | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Exercise not found
stats-exercise-bob.golden.html | GET /stats/exercise/22a1bd31-38fc-53db-9ce2-9d43243ea063 | bob | 200
settings.golden.html | GET /settings | alice | 200
settings-timezone-set.golden.html | GET /settings | bob | 200
settings-timezone-error.golden.html | POST /settings/timezone [timezone=Mars/Olympus] | alice | 400
settings-timezone-case.golden.html | POST /settings/timezone [timezone=asia/taipei] | alice | 400
settings-timezone-updated.golden.html | POST /settings/timezone [timezone=America/New_York] | carol | 200
settings-password-mismatch.golden.html | POST /settings/password [current_password=x&new_password=correct+horse+battery+staple&confirm_password=different+horse+battery] | alice | 200
settings-password-same.golden.html | POST /settings/password [current_password=correct+horse+battery+staple&new_password=correct+horse+battery+staple&confirm_password=correct+horse+battery+staple] | alice | 200
settings-password-short.golden.html | POST /settings/password [current_password=correct+horse+battery+staple&new_password=short&confirm_password=short] | alice | 200
settings-password-weak.golden.html | POST /settings/password [current_password=correct+horse+battery+staple&new_password=password1234&confirm_password=password1234] | alice | 200
settings-password-wrong-current.golden.html | POST /settings/password [current_password=not+the+password&new_password=x7%23Kp2%24mQ9vL&confirm_password=x7%23Kp2%24mQ9vL] | alice | 200
users.golden.html | GET /users | alice | 200
users-as-user.golden.html | GET /users | bob | 200
users-new.golden.html | GET /users/new | alice | 200
(no golden) users-new-as-user | GET /users/new | bob | 403 | ctype=text/plain; charset=utf-8 | location= | body=Admin access required
users-new-weak.golden.html | POST /users/new [username=dave&password=password1234] | alice | 200
users-new-short.golden.html | POST /users/new [username=dave&password=short] | alice | 200
users-new-no-username.golden.html | POST /users/new [username=+++&password=x7%23Kp2%24mQ9vL] | alice | 200
users-new-duplicate.golden.html | POST /users/new [username=bob&password=x7%23Kp2%24mQ9vL] | alice | 200
users-new-weak-with-username.golden.html | POST /users/new [username=davedave&password=davedave1234] | alice | 200
confirm-promote-user.golden.html | GET /users/244f6aef-a7c9-5119-8ae6-7d18f318df5d/promote | alice | 200
confirm-delete-user.golden.html | GET /users/244f6aef-a7c9-5119-8ae6-7d18f318df5d/delete | alice | 200
(no golden) confirm-delete-user-self | GET /users/b6db7e65-fdaf-5a9b-ad5c-644b892fafd3/delete | alice | 400 | ctype=text/plain; charset=utf-8 | location= | body=Cannot delete your own account
(no golden) confirm-delete-user-unknown | GET /users/nope/delete | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=User not found
(no golden) confirm-promote-user-as-user | GET /users/244f6aef-a7c9-5119-8ae6-7d18f318df5d/promote | bob | 403 | ctype=text/plain; charset=utf-8 | location= | body=Admin access required
confirm-promote-user-wrong-password.golden.html | POST /users/244f6aef-a7c9-5119-8ae6-7d18f318df5d/promote [current_password=wrong] | alice | 200
confirm-delete-user-wrong-password.golden.html | POST /users/244f6aef-a7c9-5119-8ae6-7d18f318df5d/delete [current_password=wrong] | alice | 200
confirm-workout-delete.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd/delete | alice | 200
confirm-workout-delete-empty.golden.html | GET /workouts/71cfcccf-b0aa-533b-84e4-55839cfbd941/delete | alice | 200
confirm-workout-delete-one-set.golden.html | GET /workouts/85eef01d-e4a4-5389-9b26-4e868e92b540/delete | bob | 200
(no golden) confirm-workout-delete-foreign | GET /workouts/c76f5408-51e6-55ce-b637-4c873b32afca/delete | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Workout not found
confirm-log-delete.golden.html | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd/logs/a1e7f98d-1592-55db-b31e-b0514388cde2/delete | alice | 200
(no golden) confirm-log-delete-foreign | GET /workouts/0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd/logs/8cc802ff-bd30-5526-8da0-4886aa207173/delete | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Set not found
confirm-revoke-share.golden.html | GET /workouts/4592a164-902e-551c-a8e9-a7cf803de5da/revoke-share | alice | 200
(no golden) confirm-revoke-share-foreign | GET /workouts/c76f5408-51e6-55ce-b637-4c873b32afca/revoke-share | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Workout not found
confirm-exercise-delete.golden.html | GET /exercises/aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea/delete | alice | 200
confirm-exercise-delete-escaped.golden.html | GET /exercises/0376c522-0917-5353-b064-fbfc9510d054/delete | alice | 200
(no golden) confirm-exercise-delete-foreign | GET /exercises/22a1bd31-38fc-53db-9ce2-9d43243ea063/delete | alice | 404 | ctype=text/plain; charset=utf-8 | location= | body=Exercise not found
confirm-logout-others.golden.html | GET /settings/logout-others | alice | 200
confirm-logout-others-none.golden.html | GET /settings/logout-others | bob | 200
(no golden) health | GET /health | none | 200 | ctype=application/json | location= | body={"status":"ok","git_version":"v0.30.1-1-g19c79d8"}
(no golden) anon-redirect-workouts | GET /workouts | none | 303 | ctype= | location=/auth/login | body=
(no golden) anon-redirect-settings | GET /settings | none | 303 | ctype= | location=/auth/login | body=
(no golden) not-found | GET /nope | none | 404 | ctype= | location= | body=
(no golden) forbidden-users-new-post | POST /users/new [username=dave&password=x7%23Kp2%24mQ9vL] | bob | 403 | ctype=text/plain; charset=utf-8 | location= | body=Admin access required

== Empty server (first-run setup; anonymous unless a cookie is noted) ==
The setup token is printed on stderr ("Setup token: XXXXX-XXXXX"); the POST bodies below show
the one used. Form: POST /auth/setup username, password, setup_token. The 5-attempt setup
limiter charges a wrong token and refunds a rejected username/password.
setup-success sets the first admin's session cookie; the later requests carry it.

(no golden) anon-redirect-login-empty-db | GET /auth/login [] | 303 | body=
setup.golden.html | GET /auth/setup [] | 200
setup-invalid-token.golden.html | POST /auth/setup [username=admin&password=correct+horse+battery+staple&setup_token=WRONG-TOKEN] | 403
setup-weak-password.golden.html | POST /auth/setup [username=admin&password=password1234&setup_token=X2ZY9-A3TJP] | 200
setup-short-password.golden.html | POST /auth/setup [username=admin&password=short&setup_token=X2ZY9-A3TJP] | 200
setup-no-username.golden.html | POST /auth/setup [username=+++&password=correct+horse+battery+staple&setup_token=X2ZY9-A3TJP] | 200
(no golden) setup-success | POST /auth/setup [username=admin&password=correct+horse+battery+staple&setup_token=X2ZY9-A3TJP] | 303 | body=
setup cookie: session=547faca9-954c-4a03-ae83-56241b5e56c9
(no golden) setup-again-redirect | GET /auth/setup [] | 303 | body=
(no golden) login-while-signed-in | GET /auth/login [] | 303 | body=
(no golden) ex-create-squat | POST /exercises [name=Squat&category=legs] | 303 | body=
(no golden) ex-create-bench | POST /exercises [name=Bench+Press&category=chest] | 303 | body=
(no golden) wk-create | POST /workouts [date=2026-11-10&notes=First+session] | 303 | body=
(no golden) log-1 | POST /workouts/5ec1073b-9cbf-45bd-86cc-80ae5afcb356/logs [exercise_id=8227bbdc-efec-45d3-b9fb-c15ed75f0df0&reps=5&weight=100&rpe=8] | 303 | body=
(no golden) log-2 | POST /workouts/5ec1073b-9cbf-45bd-86cc-80ae5afcb356/logs [exercise_id=8227bbdc-efec-45d3-b9fb-c15ed75f0df0&reps=5&weight=102.5&rpe=] | 303 | body=
(no golden) log-3 | POST /workouts/5ec1073b-9cbf-45bd-86cc-80ae5afcb356/logs [exercise_id=af6b51cc-a27f-4cd0-a849-5931f64a2d8a&reps=8&weight=60&rpe=7] | 303 | body=
(no golden) share | POST /workouts/5ec1073b-9cbf-45bd-86cc-80ae5afcb356/share [expires_in_days=30] | 303 | body=
(no golden) new-user | POST /users/new [username=bob&password=x7%23Kp2%24mQ9vL] | 303 | body=
workout-show-created.golden.html | GET /workouts/5ec1073b-9cbf-45bd-86cc-80ae5afcb356 [] | 200
(no golden) login-success | POST /auth/login [username=bob&password=x7%23Kp2%24mQ9vL] | 303 | body=
(no golden) logout | POST /auth/logout [] | 303 | body=
