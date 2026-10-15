//! Opencord client core, exposed to the Flutter app through flutter_rust_bridge.

pub mod api;
pub mod client;
mod connection;
mod convert;
mod ducking;
#[rustfmt::skip]
mod frb_generated;
mod http;
pub mod identity;
pub mod media;
pub mod mirror;
mod soundboard;
pub mod store;
pub mod tofu;
pub mod video;
pub mod voice;
