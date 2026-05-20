//! Library facade — exposes modules so secondary binaries (`verify`,
//! `scheduler`) can re-use scraper / mailer / config code without
//! duplicating modules. The main TUI bot (`src/main.rs`) keeps its
//! own private module tree and is not affected.

pub mod config;
pub mod error;
pub mod gmail;
pub mod history;
pub mod mailer;
pub mod queue;
pub mod scraper;
