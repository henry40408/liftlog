//! The Cucumber runner.
//!
//! `harness = false`: cucumber drives the scenarios itself, so there is no
//! libtest harness collecting `#[test]` functions. Run it with
//! `cargo test --test e2e` from `e2e/`.
//!
//! What `playwright.config.js` expressed as one project over parallel workers
//! is expressed here as two sequential passes over one server:
//!
//! * `@bootstrap` — the first-run scenarios assert on an install with no users
//!   in it, which stops being true the moment any other scenario seeds its
//!   admin. They run first, against the empty database the server just created,
//!   and none of them creates an account: two submit passwords the policy
//!   refuses, and the third only follows a redirect.
//! * everything else — one browser per scenario, each seeding and signing in
//!   for itself.
//!
//! The old suite got this for free by giving every Playwright worker its own
//! database, and paid for it with a server process per worker.

mod steps;

use cucumber::World as _;
use cucumber::gherkin;
use cucumber::writer::Stats as _;
use liftlog_e2e::Server;
use liftlog_e2e::browser::Browser;
use liftlog_e2e::world::LiftLogWorld;

const FEATURES: &str = "features";

/// The most scenarios — and so browsers — to run at once, whatever the machine.
const CONCURRENCY_CEILING: usize = 4;

/// How many scenarios run at once, one per core up to [`CONCURRENCY_CEILING`].
///
/// A fixed four is too many for a two-core CI runner, where four browsers
/// contend for two cores until pages take longer to settle than the steps wait
/// for — the kind of flake that is worse than being slow.
fn max_concurrent_scenarios() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(CONCURRENCY_CEILING)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Killed when this binding drops at the end of `main`.
    let _server = Server::start().await?;
    // Before anything runs in parallel — see `Browser::prepare`.
    Browser::prepare().await?;

    let bootstrap = run(|feature, _, scenario| tagged(feature, scenario, "bootstrap")).await;
    let rest = run(|feature, _, scenario| !tagged(feature, scenario, "bootstrap")).await;

    // Both passes run before either can fail the process: a broken first-run
    // flow is worth seeing on the same run that showed the rest passing.
    let failures = bootstrap + rest;
    anyhow::ensure!(failures == 0, "{failures} cucumber failure(s)");
    Ok(())
}

/// Runs the scenarios a filter selects, reporting how many ways it failed.
async fn run<F>(filter: F) -> usize
where
    F: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> bool + 'static,
{
    let writer = LiftLogWorld::cucumber()
        .max_concurrent_scenarios(max_concurrent_scenarios())
        .fail_on_skipped()
        .before(|_feature, _rule, _scenario, world| {
            Box::pin(async move {
                world
                    .open()
                    .await
                    .expect("could not open a browser session");
            })
        })
        .after(|_feature, _rule, _scenario, _finished, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.close().await.expect("could not close the session");
                }
            })
        })
        .filter_run(FEATURES, filter)
        .await;

    writer.failed_steps() + writer.parsing_errors() + writer.hook_errors()
}

/// Is the scenario tagged, either directly or through its feature?
///
/// `gherkin` does not propagate a feature-level tag onto the scenarios beneath
/// it, so a filter that only reads `scenario.tags` silently selects nothing —
/// which put the first-run scenarios in the second pass, against a database
/// that by then had an admin in it.
fn tagged(feature: &gherkin::Feature, scenario: &gherkin::Scenario, tag: &str) -> bool {
    let carries = |tags: &[String]| tags.iter().any(|candidate| candidate == tag);
    carries(&feature.tags) || carries(&scenario.tags)
}
