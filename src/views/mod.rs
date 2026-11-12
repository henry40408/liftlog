//! Server-rendered pages, one module per area. Each page is a plain function
//! from its view data to `maud::Markup`, composed inside `layout::page`.

pub mod auth;
pub mod confirm;
pub mod dashboard;
pub mod exercises;
pub mod layout;
pub mod settings;
pub mod stats;
pub mod workout_show;
pub mod workouts;
