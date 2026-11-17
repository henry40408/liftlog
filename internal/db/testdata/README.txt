rust-created.sqlite3: a database written end to end by the Rust binary (commit 19c79d8, all 11
migrations applied, WAL checkpointed so it is a single file). The Go server must open it as is,
log in with the stored argon2id hashes, read the stored timestamps and keep `_migrations`
unchanged. Created on 2026-11-16 ~16:38 UTC, through the HTTP API of the empty server:

  admin   id 4e85b172-fa25-4a95-81b6-8df5f8911cf7   role admin   timezone NULL
          password: correct horse battery staple     (set up through /auth/setup)
  bob     id 69da6614-050d-46f7-8630-68cc4f4d5230   role user    timezone NULL
          password: x7#Kp2$mQ9vL

  sessions  one row: token 547faca9-954c-4a03-ae83-56241b5e56c9 (admin, from the setup
            sign-in), created 2026-11-16 16:38:55.725186+00:00, expires 2026-11-23
            16:38:55.725186+00:00. A login by bob and its logout left no row. Sessions
            expire after 2026-11-23, so a test must not assume it is still valid; it may
            insert its own session row for admin, or check expiry handling instead.
  exercises Squat (legs) 8227bbdc-efec-45d3-b9fb-c15ed75f0df0,
            Bench Press (chest) af6b51cc-a27f-4cd0-a849-5931f64a2d8a   (both admin's)
  workout   5ec1073b-9cbf-45bd-86cc-80ae5afcb356  admin, date 2026-11-10, notes "First session"
            logs: Squat set 1 5x100.0 rpe 8, Squat set 2 5x102.5 no rpe, Bench Press set 1
            8x60.0 rpe 7 (weights are stored as REAL, e.g. 100.0)
  share     the workout above: share_token 68c9c4b1-805f-4561-abcd-96e40ae2f67d,
            share_expires_at 2026-12-16 16:38:56.097583+00:00 (30 days after creation;
            expired after that date, so /shared/<token> is 404 from 2026-12-17 on)

Timestamps are exactly what chrono + rusqlite wrote: `YYYY-MM-DD HH:MM:SS.ffffff+00:00` here
(6 fractional digits, because macOS gives microsecond precision; Linux writes 9 digits and a
whole second writes none). Dates are `YYYY-MM-DD`. See the plan's parity hazard 1.
