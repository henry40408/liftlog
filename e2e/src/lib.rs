//! Browser E2E support for LiftLog: the server under test, the browser session,
//! the fixture seeding, and the page objects the Cucumber steps drive.

pub mod browser;
pub mod http;
pub mod pages;
pub mod seeding;
pub mod server;
pub mod wait;
pub mod world;

pub use server::Server;
