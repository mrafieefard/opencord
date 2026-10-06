//! Opencord client core, exposed to the Flutter app through flutter_rust_bridge.

pub mod api;
pub mod client;
mod connection;
mod convert;
#[rustfmt::skip]
mod frb_generated;
pub mod identity;
pub mod mirror;
pub mod store;
pub mod tofu;
