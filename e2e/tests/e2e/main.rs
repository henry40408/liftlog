//! The Cucumber runner (`harness = false`). Two sequential passes over one
//! server: `@bootstrap` first, on the empty database, then everything else.

mod steps;

use cucumber::World as _;
use cucumber::gherkin;
use cucumber::writer::Stats as _;
use liftlog_e2e::Server;
use liftlog_e2e::browser::Browser;
use liftlog_e2e::world::LiftLogWorld;

const FEATURES: &str = "features";

const CONCURRENCY_CEILING: usize = 4;

/// One scenario per core, capped: four browsers on a two-core runner settle
/// slower than the steps wait.
fn max_concurrent_scenarios() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(CONCURRENCY_CEILING)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _server = Server::start().await?;
    // Before anything runs in parallel — see `Browser::prepare`.
    Browser::prepare().await?;

    let bootstrap = run(|feature, _, scenario| tagged(feature, scenario, "bootstrap")).await;
    let rest = run(|feature, _, scenario| !tagged(feature, scenario, "bootstrap")).await;

    // Run both passes before failing, so one run reports everything.
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

/// Is the scenario tagged, directly or via its feature? `gherkin` does not
/// propagate feature tags onto scenarios.
fn tagged(feature: &gherkin::Feature, scenario: &gherkin::Scenario, tag: &str) -> bool {
    let carries = |tags: &[String]| tags.iter().any(|candidate| candidate == tag);
    carries(&feature.tags) || carries(&scenario.tags)
}
