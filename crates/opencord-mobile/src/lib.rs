//! Opencord's client core for the phone apps (design
//! docs/superpowers/specs/2026-10-26-mobile-client-design.md §5).
//!
//! A UniFFI facade over [`opencord_core::client::Client`], the same client the
//! Flutter API wraps. JavaScript gets what it handles well: ids as decimal
//! strings, times and sizes as numbers, permissions as named flags, and
//! events it pulls in batches with `next_events`.

uniffi::setup_scaffolding!();

pub mod api;
mod convert;
mod error;
mod events;
mod files;
mod ids;
mod mobile;
mod types;

pub use error::MobileError;
pub use events::MobileEvent;
pub use ids::{id, parse_id};
pub use mobile::Mobile;
pub use types::*;
