//! Opencord client core, exposed to the Flutter app through flutter_rust_bridge.

pub mod api;
mod artwork;
pub mod attachments;

pub mod client;
mod commands;
mod connection;
mod convert;
mod ducking;
#[rustfmt::skip]
mod frb_generated;
mod http;
pub mod identity;
mod lavalink;
pub mod media;
pub mod mirror;
mod music;
pub(crate) mod music_listening;
mod network;
pub(crate) mod now_playing;
mod server_clock;
mod soundboard;
pub mod store;
pub mod tofu;
pub mod video;
pub mod voice;
pub mod voice_message;
