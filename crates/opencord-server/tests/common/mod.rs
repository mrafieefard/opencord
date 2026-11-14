//! A real server on a random port, and a protocol-level test client.

#![allow(dead_code)]

pub mod domains;
pub use opencord_server::testing::fake_lavalink;
pub mod music;
pub mod pebble;

pub use opencord_server::testing::server::*;
